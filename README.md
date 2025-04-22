# pyo3-twisted

An experimental bridge between Tokio and Twisted Deferreds.

#### Trying out

There is an example of usage in the [`pyo3-twiste-example`](./pyo3-twisted-example/) directory.

The Rust code exposes a few async functions like this:

```rust
#[pyfunction]
fn rusty_sleep(reactor: Bound<PyAny>, seconds: u64) -> PyResult<Bound<PyAny>> {
    pyo3_twisted::async_fn_into_py(reactor, async move || {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        Ok(())
    })
}
```

Assuming [`uv`](https://docs.astral.sh/uv/) and [a Rust compiler](https://www.rust-lang.org/learn/get-started) are available, run this example with:

```bash
uv run pyo3-twisted-example
```
