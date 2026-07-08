#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RrlDecision {
    Send,
    Drop,
    Slip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseClass {
    NoError,
    NxDomain,
    Error,
}

impl ResponseClass {
    pub fn from_rcode(rcode: u8) -> Self {
        match rcode {
            0 => ResponseClass::NoError,
            3 => ResponseClass::NxDomain,
            _ => ResponseClass::Error,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ResponseClass::NoError => "noerror",
            ResponseClass::NxDomain => "nxdomain",
            ResponseClass::Error => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_classes_map_from_rcodes() {
        assert_eq!(ResponseClass::from_rcode(0), ResponseClass::NoError);
        assert_eq!(ResponseClass::from_rcode(3), ResponseClass::NxDomain);
        assert_eq!(ResponseClass::from_rcode(2), ResponseClass::Error); // SERVFAIL
        assert_eq!(ResponseClass::from_rcode(5), ResponseClass::Error); // REFUSED
    }
}
