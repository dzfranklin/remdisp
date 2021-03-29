macro_rules! unavailable {
    ($($arg:tt)*) => {{
        Status::unavailable(format!($($arg)*))
    }}
}
