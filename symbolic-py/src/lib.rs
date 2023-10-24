use pyo3::prelude::*;

mod proguard;

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

/// A Python module implemented in Rust.
#[pymodule]
fn symbolic(py: Python, m: &PyModule) -> PyResult<()> {
    // FIXME: https://pyo3.rs/v0.20.0/module#python-submodules
    let proguard_module = PyModule::new(py, "proguard")?;
    proguard_module.add_class::<proguard::JavaStackFrame>()?;
    proguard_module.add_class::<proguard::ProguardMapper>()?;
    m.add_submodule(proguard_module)?;

    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    Ok(())
}
