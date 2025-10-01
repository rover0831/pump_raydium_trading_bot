use chrono::{DateTime, Utc};

/// Generate a random string of specified length
pub fn generate_random_string(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789)(*&^%$#@!~";
    let mut rng = rand::thread_rng();
    
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Format timestamp for display
pub fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Validate email format
pub fn is_valid_email(email: &str) -> bool {
    use regex::Regex;
    lazy_static::lazy_static! {
        static ref EMAIL_REGEX: Regex = Regex::new(
            r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$"
        ).unwrap();
    }
    EMAIL_REGEX.is_match(email)
}

/// Sanitize input string
pub fn sanitize_input(input: &str) -> String {
    input.trim().to_string()
}

/// Check if string contains only alphanumeric characters
pub fn is_alphanumeric(input: &str) -> bool {
    input.chars().all(|c| c.is_alphanumeric())
}
