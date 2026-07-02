use crate::types::Name;

#[derive(Debug, Clone)]
pub struct Question {
    pub name: Name,
    pub qtype: u16,
    pub qclass: u16,
}