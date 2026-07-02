use crate::dns_message::response_codes::ResponseCode;

#[derive(Debug, Clone)]
pub struct Header {
    pub id: u16,
    pub qr: bool, pub opcode: u8, pub aa: bool, pub tc: bool, pub rd: bool,
    pub ra: bool, pub z: u8, pub rcode: ResponseCode,
    pub qdcount: u16, pub ancount: u16, pub nscount: u16, pub arcount: u16,
}

impl Header {
    pub fn flags(&self) -> u16 {
        ((self.qr as u16) << 15)
        | (((self.opcode & 0x0F) as u16) << 11)
        | ((self.aa as u16) << 10)
        | ((self.tc as u16) << 9)
        | ((self.rd as u16) << 8)
        | ((self.ra as u16) << 7)
        | (((self.z & 0x07) as u16) << 4)
        | ((u8::from(self.rcode) & 0x0F) as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(qr: bool, opcode: u8, aa: bool, tc: bool, rd: bool, ra: bool, z: u8, rcode: ResponseCode) -> Header {
        Header {
            id: 0,
            qr, opcode, aa, tc, rd, ra, z, rcode,
            qdcount: 0, ancount: 0, nscount: 0, arcount: 0,
        }
    }

    #[test]
    fn flags_standard_query_is_zero() {
        assert_eq!(header(false, 0, false, false, false, false, 0, ResponseCode::NoError).flags(), 0x0000);
    }

    #[test]
    fn flags_typical_response() {
        // qr=1, rd=1, ra=1 => 1000_0001_1000_0000
        assert_eq!(header(true, 0, false, false, true, true, 0, ResponseCode::NoError).flags(), 0x8180);
    }

    #[test]
    fn flags_pack_each_field_into_correct_bit() {
        assert_eq!(header(true, 0, false, false, false, false, 0, ResponseCode::NoError).flags(), 1 << 15);
        assert_eq!(header(false, 0xF, false, false, false, false, 0, ResponseCode::NoError).flags(), 0xF << 11);
        assert_eq!(header(false, 0, true, false, false, false, 0, ResponseCode::NoError).flags(), 1 << 10);
        assert_eq!(header(false, 0, false, true, false, false, 0, ResponseCode::NoError).flags(), 1 << 9);
        assert_eq!(header(false, 0, false, false, true, false, 0, ResponseCode::NoError).flags(), 1 << 8);
        assert_eq!(header(false, 0, false, false, false, true, 0, ResponseCode::NoError).flags(), 1 << 7);
        assert_eq!(header(false, 0, false, false, false, false, 0x7, ResponseCode::NoError).flags(), 0x7 << 4);
        // rcode is a bounded enum; NotZone (10) is the largest variant.
        assert_eq!(header(false, 0, false, false, false, false, 0, ResponseCode::NotZone).flags(), 10);
    }

    #[test]
    fn flags_masks_oversized_fields() {
        // opcode and z are masked to their bit widths, so overflow bits don't leak.
        assert_eq!(header(false, 0xFF, false, false, false, false, 0xFF, ResponseCode::NoError).flags(),
                   (0xF << 11) | (0x7 << 4));
    }
}
