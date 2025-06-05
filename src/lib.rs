pub mod args;
pub mod backends;
pub mod config;
pub mod manifest;
pub mod paths;
pub mod platform;
pub mod secrets;
pub mod sync;
pub mod ui;

#[macro_export]
macro_rules! time {
    ($name:literal : { $($code:tt)* }) => {
        let time_start = SystemTime::now();
        $($code)*
        let time_end = SystemTime::now();
        ::tracing::debug!(
            "{} took {}ms", $name,
            time_end.duration_since(time_start)?.as_millis()
        );
    };
}

pub fn curr_crate_ver() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("failed to parse crate version??")
}
