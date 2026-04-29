//! Static asset embedding. Files in `src/web/static/` are embedded at
//! build time via `include_str!`.

pub const APP_CSS: &str = include_str!("static/app.css");
pub const APP_JS: &str = include_str!("static/app.js");
pub const DASHBOARD_HTML: &str = include_str!("static/dashboard.html");
pub const SESSION_HTML: &str = include_str!("static/session.html");
