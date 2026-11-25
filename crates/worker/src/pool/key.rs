use crate::registry::RuntimeKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolKey {
    pub runtime: RuntimeKind,
    pub artifact: String,
}
