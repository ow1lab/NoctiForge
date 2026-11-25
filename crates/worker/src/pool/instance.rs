use runtime::Instance;
use tokio::time::Instant;

pub struct IdleInstance {
    pub instance: Box<dyn Instance>,
    pub last_used: Instant,
}
