use once_cell::sync::OnceCell;
use pyo3::IntoPyObjectExt;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::marker::Ungil;
use pyo3::prelude::*;
use tokio::sync::oneshot::Sender;

/// A reference to the `twisted.internet.defer` module.
static DEFER: OnceCell<PyObject> = OnceCell::new();
/// A reference to the `contextvars` module.
static CONTEXTVARS: OnceCell<PyObject> = OnceCell::new();
/// A lazy-initialized [`tokio::runtime::Runtime`].
static RUNTIME: OnceCell<tokio::runtime::Runtime> = OnceCell::new();

create_exception!(
    pyo3_twisted._core,
    RustPanic,
    PyException,
    "A panic which happened in a Rust future"
);

/// Access to the `twisted.internet.defer` module.
fn defer(py: Python) -> PyResult<&Bound<PyAny>> {
    Ok(DEFER
        .get_or_try_init(|| py.import("twisted.internet.defer").map(Into::into))?
        .bind(py))
}

/// Get a reference to a lazy-initialized [`tokio::runtime::Runtime`].
fn runtime() -> PyResult<&'static tokio::runtime::Runtime> {
    Ok(RUNTIME.get_or_try_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
    })?)
}

/// Access to the `contextvars` module.
fn contextvars(py: Python) -> PyResult<&Bound<PyAny>> {
    Ok(CONTEXTVARS
        .get_or_try_init(|| py.import("contextvars").map(Into::into))?
        .bind(py))
}

/// A function that sends a signal through a channel when called
#[pyclass]
struct PyCancelTx {
    tx: Option<Sender<()>>,
}

#[pymethods]
impl PyCancelTx {
    #[pyo3(signature = (deferred))]
    pub fn __call__(&mut self, deferred: &Bound<'_, PyAny>) {
        // We *could* be calling `Deferred.errback` here, but `Deferred.cancel`
        // already does that for us. It saves us the hassle of making sure we do
        // that from the right thread.
        let _ = deferred;

        // Send the signal through the channel to cancel the future
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Set the result of a Twisted Deferred.
///
/// The Deferred callback/errback will be called in the context of the Twisted
/// reactor thread, with the contextvars restored.
///
/// # Parameters
///
///  - `reactor` implements `twisted.internet.interfaces.IReactorCore`
///  - `context` is a `contextvars.Context`
///  - `deferred` is a `twisted.internet.defer.Deferred`
///  - `result` is the result of the Rust future
fn set_result(
    reactor: &Bound<PyAny>,
    context: &Bound<PyAny>,
    deferred: &Bound<PyAny>,
    result: PyResult<PyObject>,
) -> PyResult<()> {
    let py = reactor.py();

    let called: bool = deferred.getattr("called")?.extract()?;
    if called {
        // `callback`/`errback` have already been called, so we can just return.
        return Ok(());
    }

    let (complete, val) = match result {
        Ok(val) => (deferred.getattr("callback")?, val.into_pyobject(py)?),
        Err(err) => (
            deferred.getattr("errback")?,
            err.into_pyobject(py)?.into_any(),
        ),
    };

    // Equivalent to `reactor.callFromThread(context.run, deferred.callback/errback,
    // result)` Using `reactor.callFromThread` so that we run the callbacks in
    // the reactor thread Using `context.run` to restore the contextvars
    let context_run = context.getattr("run")?;
    reactor.call_method("callFromThread", (context_run, complete, val), None)?;

    Ok(())
}

/// Call a Rust async function into a Twisted Deferred.
///
/// # Errors
///
/// This function will return a [`PyErr`] if:
///
///  - the runtime could not be initialized
///  - `contectvars.copy_context` failed
///  - `twisted.internet.defer.Deferred()` failed
pub fn async_fn_into_py<F, Fut, T>(reactor: Bound<PyAny>, f: F) -> PyResult<Bound<PyAny>>
where
    F: FnOnce() -> Fut + Ungil,
    Fut: Future<Output = PyResult<T>> + Send + 'static,
    T: for<'py> IntoPyObject<'py> + Send + 'static,
{
    // Enter the runtime context, in case `tokio::spawn` or other are used during
    // the future creation
    let _guard = runtime()?.enter();
    let py = reactor.py();

    // Create the future, releasing the GIL during that operation
    // TODO: we should catch panics here, and return a Twisted failure
    let future = py.allow_threads(f);

    // Transform the Future into a Deferred
    future_into_py(reactor, future)
}

fn future_into_py<F, T>(reactor: Bound<PyAny>, fut: F) -> PyResult<Bound<PyAny>>
where
    F: Future<Output = PyResult<T>> + Send + 'static,
    T: for<'py> IntoPyObject<'py> + Send + 'static,
{
    let py = reactor.py();

    // Get a reference to the runtime
    let rt = runtime()?;

    // Copy the current context.
    let context = contextvars(py)?.call_method0("copy_context")?;

    // Create a new deferred and a channel that will fire when the deferred is
    // cancelled.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let bound_deferred = defer(py)?.call_method1("Deferred", (PyCancelTx { tx: Some(tx) },))?;

    // Spawn the future
    let handle = rt.spawn(fut);

    // Spawn a task that will fire when the future is cancelled.
    let abort_handle = handle.abort_handle();
    rt.spawn(async move {
        if let Ok(()) = rx.await {
            abort_handle.abort();
        }
    });

    // Unbind the reactor and the deferred so that we can release the GIL.
    let reactor = reactor.unbind();
    let context = context.unbind();
    let deferred = bound_deferred.clone().unbind();

    // Now spawn a task which waits for the future to complete and get the result
    rt.spawn(async move {
        let result = handle.await;

        // We got the result, let's get the GIL back
        Python::with_gil(move |py| {
            // Re-bind the objects to the current Python context.
            let reactor = reactor.bind(py);
            let deferred = deferred.bind(py);
            let context = context.bind(py);

            let result: PyResult<PyObject> = match result {
                Ok(Ok(result)) => result.into_py_any(py),
                Ok(Err(err)) => Err(err),
                Err(err) => {
                    match err.try_into_panic() {
                        Ok(panic_) => {
                            // Apparently this is how you extract the panic message from a panic
                            let message = if let Some(str_slice) = panic_.downcast_ref::<&str>() {
                                str_slice
                            } else if let Some(string) = panic_.downcast_ref::<String>() {
                                string
                            } else {
                                "unknown error"
                            };

                            let message = format!("rust future panicked: {message}");
                            Err(RustPanic::new_err(message))
                        }

                        Err(e) => {
                            // This should then be a cancellation error
                            debug_assert!(e.is_cancelled());

                            // `Deferred.cancel` will already have called 'errback'
                            // on the deferred, so don't need to do anything here
                            return;
                        }
                    }
                }
            };

            if let Err(e) = set_result(reactor, context, deferred, result) {
                // This is a last resort error handler, which uses `sys.excepthook` and prints
                // to stderr.
                e.print_and_set_sys_last_vars(py);
            }
        });
    });

    Ok(bound_deferred)
}
