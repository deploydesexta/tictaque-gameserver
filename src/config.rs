pub struct Config {
    pub redis_url: String,
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> Self {
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
        let bind_addr = format!("0.0.0.0:{}", port);

        Self { redis_url, bind_addr }
    }
}
