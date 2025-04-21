use std::time::Duration;

use pyo3::prelude::*;
use pyo3_twisted::async_fn_into_py;

#[pyfunction]
fn rusty_sleep(reactor: Bound<PyAny>, seconds: u64) -> PyResult<Bound<PyAny>> {
    async_fn_into_py(reactor, async move || {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        Ok(())
    })
}

#[pyfunction]
fn rusty_panic(reactor: Bound<PyAny>) -> PyResult<Bound<PyAny>> {
    async_fn_into_py(reactor, async || -> PyResult<()> {
        panic!("Oh, no!");
    })
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rusty_sleep, m)?)?;
    m.add_function(wrap_pyfunction!(rusty_panic, m)?)?;
    Ok(())
}
