use std::time::Instant;

use runtime::Instance;

pub struct IdleInstance {
    pub instance: Box<dyn Instance>,
    pub last_used: Instant,
}
