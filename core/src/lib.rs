pub mod auth_inject;
pub mod error;
pub mod http_client;
pub mod infer_bridge;
pub mod mcp_server;
pub mod openapi;
pub mod py;
pub mod rate_limit;
pub mod retry;
pub mod schema;

use pyo3::prelude::*;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py::parse_openapi_url, m)?)?;
    m.add_function(wrap_pyfunction!(py::parse_openapi_str, m)?)?;
    m.add_function(wrap_pyfunction!(py::infer_from_html_py, m)?)?;
    m.add_function(wrap_pyfunction!(py::schema_from_json, m)?)?;
    m.add_function(wrap_pyfunction!(py::schema_to_json, m)?)?;
    m.add_function(wrap_pyfunction!(py::start_mcp_server, m)?)?;
    m.add_class::<py::ToolSchemaPy>()?;
    m.add_class::<py::MethodPy>()?;
    m.add_class::<py::McpServerHandle>()?;
    Ok(())
}
