use crate::dns_message::header::Header;
use crate::dns_message::question::Question;
use crate::dns_message::response_codes::ResponseCode;
use crate::types::Name;
use crate::zone::resource_record::ResourceRecord;

#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid label length: {0}")]
    InvalidLabelLength(u8),
    #[error("invalid domain name")]
    InvalidDomainName,
    #[error("invalid pointer offset: {0}")]
    InvalidPointerOffset(u16),
}

impl ReaderError {
    pub fn to_response_code(&self) -> ResponseCode {
        match self {
            ReaderError::UnexpectedEof => ResponseCode::FormErr,
            ReaderError::InvalidLabelLength(_) => ResponseCode::FormErr,
            ReaderError::InvalidDomainName => ResponseCode::FormErr,
            ReaderError::InvalidPointerOffset(_) => ResponseCode::FormErr,
        }
    }
}

pub(crate) type ReaderResult<T> = Result<T, ReaderError>;

pub struct Reader<'a> {
    buffer: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buffer: &'a [u8]) -> Self { Reader { buffer, position: 0 } }

    pub fn new_with_position(buffer: &'a [u8], position: usize) -> Self {
        Reader { buffer, position }
    }

    pub fn position(&self) -> usize { self.position }

    pub fn u8(&mut self) -> ReaderResult<u8> {
        let byte = *self.buffer.get(self.position).ok_or(ReaderError::UnexpectedEof)?;
        self.position += 1;
        Ok(byte)
    }

    pub fn u16(&mut self) -> ReaderResult<u16> {
        Ok(((self.u8()? as u16) << 8) | (self.u8()? as u16))
    }

    pub fn u32(&mut self) -> ReaderResult<u32> {
        Ok(((self.u16()? as u32) << 16) | (self.u16()? as u32))
    }

    pub fn read_name(&mut self) -> ReaderResult<Name> {

        const MAX_NAME_LEN: usize = 255;

        let mut label_buffer: Vec<u8> = Vec::new();
        let mut position = self.position; // local walker, independent of self.position
        let mut jumped_to_offset = false;
        let mut name_len: usize = 0;

        loop {
            let len = *self.buffer.get(position).ok_or(ReaderError::UnexpectedEof)?;

            match len & 0xC0 {
                0x00 => {
                    position += 1; // consume length byte

                    if len == 0 {
                        label_buffer.push(0);
                        break;
                    }

                    name_len += len as usize + 1;
                    if name_len > MAX_NAME_LEN { return Err(ReaderError::InvalidDomainName) }

                    let end = position + len as usize;
                    let label = self.buffer.get(position..end).ok_or(ReaderError::UnexpectedEof)?;

                    label_buffer.push(len);
                    label_buffer.extend_from_slice(label);

                    position = end;
                }
                0xC0 => {
                    let b2 = *self.buffer.get(position + 1).ok_or(ReaderError::UnexpectedEof)?;
                    let offset = (((len & 0x3F) as usize) << 8) | b2 as usize;

                    if position <= offset {
                        return Err(ReaderError::InvalidPointerOffset(offset as u16));
                    }

                    if !jumped_to_offset {
                        self.position = position + 2;
                    }

                    position = offset;
                    jumped_to_offset = true;
                }
                _ => return Err(ReaderError::InvalidLabelLength(len))
            }
        }

        if !jumped_to_offset { self.position = position; }

        Ok(Name::new(label_buffer))
    }

    pub fn read_header(&mut self) -> ReaderResult<Header> {
        let id = self.u16()?;
        let flags = self.u16()?;
        let qdcount = self.u16()?;
        let ancount = self.u16()?;
        let nscount = self.u16()?;
        let arcount = self.u16()?;

        let qr = (flags >> 15) & 1 == 1;
        let opcode = ((flags >> 11) & 0xF) as u8;
        let aa = (flags >> 10) & 1 == 1;
        let tc = (flags >> 9) & 1 == 1;
        let rd = (flags >> 8) & 1 == 1;
        let ra = (flags >> 7) & 1 == 1;
        let z = ((flags >> 4) & 0x7) as u8;
        let rcode = ResponseCode::try_from((flags & 0xF) as u8).unwrap_or(ResponseCode::FormErr);

        Ok(Header {
            id, qr, opcode, aa, tc, rd, ra, z, rcode,
            qdcount, ancount, nscount, arcount,
        })
    }

    fn read_payload_entry_header(&mut self) -> ReaderResult<(Name, u16, u16)> {
        let name = self.read_name()?;
        let entry_type = self.u16()?;
        let class = self.u16()?;
        
        Ok((name, entry_type, class))
    }

    pub fn read_question(&mut self) -> ReaderResult<Question> {
        let (name, qtype, qclass) = self.read_payload_entry_header()?;
        
        Ok(Question { name, qtype, qclass })
    }

    pub fn read_record(&mut self) -> ReaderResult<ResourceRecord> {
        let (name, rtype, rclass) = self.read_payload_entry_header()?;
        let ttl = self.u32()?;
        let rdlength = self.u16()? as usize;

        let data_offset = self.position;
        self.position += rdlength;

        Ok(ResourceRecord::new(name, rtype, rclass, ttl, data_offset, self.buffer.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::name;

    #[test]
    fn u8_read() -> ReaderResult<()> {
        let data = [0x12];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.u8()?, 0x12);
        assert!(matches!(reader.u8(), Err(ReaderError::UnexpectedEof)));
        Ok(())
    }

    #[test]
    fn u16_read() -> ReaderResult<()> {
        let data = [0x12, 0x34];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.u16()?, 0x1234);
        assert!(matches!(reader.u16(), Err(ReaderError::UnexpectedEof)));
        Ok(())
    }

    #[test]
    fn u32_read() -> ReaderResult<()> {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.u32()?, 0x12345678);
        assert!(matches!(reader.u32(), Err(ReaderError::UnexpectedEof)));
        Ok(())
    }

    #[test]
    fn position_tracks_consumed_bytes() -> ReaderResult<()> {
        let data = [0x00, 0x00, 0x00];
        let mut reader = Reader::new_with_position(&data, 1);
        assert_eq!(reader.position(), 1);
        reader.u8()?;
        assert_eq!(reader.position(), 2);
        Ok(())
    }

    #[test]
    fn read_name_empty() -> ReaderResult<()> {
        let data = [0];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.read_name()?, name(&[]));
        Ok(())
    }

    #[test]
    fn read_name_single_label() -> ReaderResult<()> {
        let data = [3, b'w', b'w', b'w', 0];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.read_name()?, name(&["www"]));
        assert_eq!(reader.position(), 5);
        Ok(())
    }

    #[test]
    fn read_name_legitimate_no_offsets() -> ReaderResult<()> {
        let data = [
            3, b'w', b'w', b'w',
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            3, b'c', b'o', b'm',
            0,
        ];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.read_name()?, name(&["www", "example", "com"]));
        Ok(())
    }

    #[test]
    fn read_name_with_offset() -> ReaderResult<()> {
        let data = [
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            3, b'c', b'o', b'm',
            0,
            3, b'w', b'w', b'w',
            0xC0, 0x00, // pointer to offset 0
        ];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.read_name()?, name(&["example", "com"]));
        assert_eq!(reader.read_name()?, name(&["www", "example", "com"]));
        // Cursor stops right after the two-byte pointer.
        assert_eq!(reader.position(), data.len());
        Ok(())
    }

    #[test]
    fn read_name_with_multiple_offsets() -> ReaderResult<()> {
        let data = [
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            3, b'c', b'o', b'm',
            0,
            3, b'w', b'w', b'w',
            0xC0, 0x00, // pointer to offset 0
            0xC0, 0x08, // pointer to offset 8 (the "com" label)
        ];
        let mut reader = Reader::new(&data);
        assert_eq!(reader.read_name()?, name(&["example", "com"]));
        assert_eq!(reader.read_name()?, name(&["www", "example", "com"]));
        assert_eq!(reader.read_name()?, name(&["com"]));
        Ok(())
    }

    #[test]
    fn read_name_with_invalid_offset() {
        let data = [3, b'w', b'w', b'w', 0xC0, 0xFF]; // pointer to offset 255 (past EOF)
        let mut reader = Reader::new(&data);
        assert!(matches!(
            reader.read_name(),
            Err(ReaderError::InvalidPointerOffset(255))
        ));
    }

    #[test]
    fn read_name_with_pointer_cycle_errors_as_invalid_domain() {
        // A pointer that points forward/at itself is rejected because position <= offset.
        let data = [3, b'w', b'w', b'w', 0xC0, 0x04]; // pointer at offset 4 -> itself
        let mut reader = Reader::new(&data);
        assert!(matches!(
            reader.read_name(),
            Err(ReaderError::InvalidPointerOffset(4))
        ));
    }

    #[test]
    fn read_name_backward_pointer_cycle_overflows_max_len() {
        // Pointer at position 4 -> offset 0 is allowed (offset < position), but re-reading
        // "www" forever overflows MAX_NAME_LEN and is reported as an invalid domain name.
        let data = [3, b'w', b'w', b'w', 0xC0, 0x00];
        let mut reader = Reader::new(&data);
        assert!(matches!(
            reader.read_name(),
            Err(ReaderError::InvalidDomainName)
        ));
    }

    #[test]
    fn read_name_with_invalid_label_length() {
        let data = [0x40, b'w', b'w', b'w']; // 0x40 is reserved
        let mut reader = Reader::new(&data);
        assert!(matches!(
            reader.read_name(),
            Err(ReaderError::InvalidLabelLength(0x40))
        ));
    }

    #[test]
    fn read_name_with_unexpected_eof() {
        let data = [3, b'w', b'w']; // label claims 3 bytes but only 2 follow
        let mut reader = Reader::new(&data);
        assert!(matches!(reader.read_name(), Err(ReaderError::UnexpectedEof)));
    }

    #[test]
    fn read_header_decodes_fields_and_flags() -> ReaderResult<()> {
        // id=0x1234, flags=0x8180 (qr=1, rd=1, ra=1), counts 1/2/3/4
        let data = [
            0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04,
        ];
        let mut reader = Reader::new(&data);
        let header = reader.read_header()?;
        assert_eq!(header.id, 0x1234);
        assert!(header.qr);
        assert!(header.rd);
        assert!(header.ra);
        assert!(!header.aa);
        assert_eq!(header.opcode, 0);
        assert_eq!(header.rcode, ResponseCode::NoError);
        assert_eq!(header.qdcount, 1);
        assert_eq!(header.ancount, 2);
        assert_eq!(header.nscount, 3);
        assert_eq!(header.arcount, 4);
        Ok(())
    }

    #[test]
    fn read_question_decodes_name_type_class() -> ReaderResult<()> {
        let data = [
            3, b'w', b'w', b'w', 3, b'c', b'o', b'm', 0, // www.com
            0x00, 0x01, // qtype = A
            0x00, 0x01, // qclass = IN
        ];
        let mut reader = Reader::new(&data);
        let q = reader.read_question()?;
        assert_eq!(q.name, name(&["www", "com"]));
        assert_eq!(q.qtype, 1);
        assert_eq!(q.qclass, 1);
        Ok(())
    }

    #[test]
    fn read_record_decodes_header_and_advances_past_rdata() -> ReaderResult<()> {
        let data = [
            3, b'c', b'o', b'm', 0, // name
            0x00, 0x01, // type A
            0x00, 0x01, // class IN
            0x00, 0x00, 0x01, 0x2C, // ttl = 300
            0x00, 0x04, // rdlength = 4
            10, 0, 0, 1, // rdata
        ];
        let mut reader = Reader::new(&data);
        let rr = reader.read_record()?;
        assert_eq!(rr.name, name(&["com"]));
        assert_eq!(rr.rtype, 1);
        assert_eq!(rr.rclass, 1);
        assert_eq!(rr.ttl, 300);
        assert_eq!(reader.position(), data.len());
        Ok(())
    }
}