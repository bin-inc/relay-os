use core::fmt;
use core::str::FromStr;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GptGuid(pub [u8; 16]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuidParseError;

impl GptGuid {
    pub const fn from_gpt_bytes(mut bytes: [u8; 16]) -> Self {
        bytes.swap(0, 3);
        bytes.swap(1, 2);
        bytes.swap(4, 5);
        bytes.swap(6, 7);
        Self(bytes)
    }

    pub const fn to_gpt_bytes(self) -> [u8; 16] {
        let mut bytes = self.0;
        bytes.swap(0, 3);
        bytes.swap(1, 2);
        bytes.swap(4, 5);
        bytes.swap(6, 7);
        bytes
    }
}

impl FromStr for GptGuid {
    type Err = GuidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value.as_bytes();
        if bytes.len() != 36
            || bytes[8] != b'-'
            || bytes[13] != b'-'
            || bytes[18] != b'-'
            || bytes[23] != b'-'
        {
            return Err(GuidParseError);
        }

        let mut guid = [0; 16];
        let mut source = 0;
        let mut destination = 0;
        while source < bytes.len() {
            if matches!(source, 8 | 13 | 18 | 23) {
                source += 1;
                continue;
            }
            let high = hex(bytes[source]).ok_or(GuidParseError)?;
            let low = hex(*bytes.get(source + 1).ok_or(GuidParseError)?).ok_or(GuidParseError)?;
            guid[destination] = (high << 4) | low;
            source += 2;
            destination += 1;
        }
        Ok(Self(guid))
    }
}

impl fmt::Display for GptGuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                formatter.write_str("-")?;
            }
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
