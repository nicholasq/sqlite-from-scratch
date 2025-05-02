#![allow(unused)]

use crate::page;
use std::{borrow::Cow, error::Error};

fn slice_to_array<const N: usize>(slice: &[u8]) -> [u8; N] {
    let mut array = [0u8; N];
    let len = slice.len().min(N);

    array[N - len..].copy_from_slice(&slice[..len]);

    array
}

/// A structure representing a variable-length integer (varint) encoding.
///
/// Varints are a method of serializing integers using one or more bytes. Smaller numbers
/// take fewer bytes. The encoding scheme is used in Protocol Buffers and other formats.
///
/// # Fields
///
/// * `value` - The decoded integer value.
/// * `size` - The number of bytes used in the encoding.
///
/// # Examples
///
/// ```
/// use sqlite_from_scratch::cell::Varint;
/// let varint = Varint { value: 300, size: 2 };
/// println!("Value: {}, encoded using {} bytes", varint.value, varint.size);
/// ```

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Varint {
    pub value: u64,
    pub size: usize,
}

impl Varint {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let mut value: u64 = 0;
        let mut size = 0;

        for (i, &byte) in bytes.iter().take(9).enumerate() {
            size += 1;
            if i == 8 {
                value = (value << 8) | (byte as u64);
            } else {
                value = (value << 7) | ((byte & 0x7F) as u64);
                if (byte & 0x80) == 0 {
                    break;
                }
            }
        }

        Ok(Varint { value, size })
    }
}

#[derive(Debug)]
pub struct CellPointer {
    pub offset: u16,
}

impl CellPointer {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        Ok(CellPointer {
            offset: u16::from_be_bytes(bytes[0..2].try_into()?),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialType {
    Null,
    Integer8,
    Integer16,
    Integer24,
    Integer32,
    Integer48,
    Integer64,
    Float64,
    Zero,
    One,
    Blob(usize),
    Text(usize),
}

impl SerialType {
    pub fn from_value(value: u64) -> Self {
        match value {
            0 => SerialType::Null,
            1 => SerialType::Integer8,
            2 => SerialType::Integer16,
            3 => SerialType::Integer24,
            4 => SerialType::Integer32,
            5 => SerialType::Integer48,
            6 => SerialType::Integer64,
            7 => SerialType::Float64,
            8 => SerialType::Zero,
            9 => SerialType::One,
            n if n >= 12 && n % 2 == 0 => SerialType::Blob(((n - 12) / 2) as usize),
            n if n >= 13 && n % 2 == 1 => SerialType::Text(((n - 13) / 2) as usize),
            _ => SerialType::Null, // Default case, could be handled differently
        }
    }

    pub fn size(&self) -> usize {
        match self {
            SerialType::Null => 0,
            SerialType::Integer8 => 1,
            SerialType::Integer16 => 2,
            SerialType::Integer24 => 3,
            SerialType::Integer32 => 4,
            SerialType::Integer48 => 6,
            SerialType::Integer64 => 8,
            SerialType::Float64 => 8,
            SerialType::Zero => 1,
            SerialType::One => 1,
            SerialType::Blob(n) => *n,
            SerialType::Text(n) => *n,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Value<'a> {
    Null,
    Integer(i64),
    // Float(f64), // let's not support floats for now
    Text(Cow<'a, str>),
    Blob(Cow<'a, [u8]>),
}

impl Value<'_> {
    fn to_owned(self) -> Value<'static> {
        match self {
            Value::Null => Value::Null,
            Value::Integer(i) => Value::Integer(i),
            Value::Text(s) => Value::Text(Cow::Owned(s.into_owned())),
            Value::Blob(b) => Value::Blob(Cow::Owned(b.into_owned())),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TableLeafCell<'a> {
    TableLeaf {
        payload_size: u64,
        rowid: u64,
        payload: Record<'a>,
        overflow_page: Option<u32>,
    },
    TableInterior {
        left_child_page: u32,
        key: u64,
    },
    IndexLeaf {
        payload_size: u64,
        payload: Record<'a>,
        overflow_page: Option<u32>,
    },
    IndexInterior {
        left_child_page: u32,
        payload_size: u64,
        payload: Record<'a>,
        overflow_page: Option<u32>,
    },
}

impl<'a> TableLeafCell<'a> {
    pub fn parse(page_type: page::BTreePageType, bytes: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        match page_type {
            page::BTreePageType::TableLeaf => Self::parse_table_leaf(bytes),
            // BTreePageType::TableInterior => Self::parse_table_interior(bytes),
            // BTreePageType::IndexLeaf => Self::parse_index_leaf(bytes),
            // BTreePageType::IndexInterior => Self::parse_index_interior(bytes),
            _ => Err("Unsupported page type")?,
        }
    }

    fn parse_table_leaf(bytes: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        let payload_size = Varint::from_bytes(bytes)?;
        let rowid = Varint::from_bytes(&bytes[payload_size.size..])?;
        let payload_start = payload_size.size + rowid.size;

        let payload = &bytes[payload_start..];
        let record = Record::parse(payload)?;

        // todo: properly calculate if the cell has an overflow_page
        let overflow_page = None;

        Ok(TableLeafCell::TableLeaf {
            payload_size: payload_size.value,
            rowid: rowid.value,
            payload: record,
            overflow_page,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct Record<'a> {
    pub header_size: usize,
    pub serial_types: Vec<SerialType>,
    pub values: Vec<Value<'a>>,
}

impl<'a> Record<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        let header_size = Varint::from_bytes(bytes)?;
        let mut offset = header_size.size;
        let mut serial_types = Vec::new();

        while offset < header_size.value as usize {
            let serial_type = Varint::from_bytes(&bytes[offset..])?;
            serial_types.push(SerialType::from_value(serial_type.value));
            offset += serial_type.size;
        }

        let mut values = Vec::new();
        let mut content_offset = header_size.value as usize;

        for serial_type in &serial_types {
            let value = Self::parse_value(serial_type, &bytes[content_offset..])?;
            content_offset += serial_type.size();
            values.push(value);
        }

        Ok(Record {
            header_size: header_size.value as usize,
            serial_types,
            values,
        })
    }

    fn parse_value(serial_type: &SerialType, bytes: &'a [u8]) -> Result<Value<'a>, Box<dyn Error>> {
        match serial_type {
            SerialType::Integer8 => {
                let a = slice_to_array::<8>(&bytes[..serial_type.size()]);
                let a = i64::from_be_bytes(a);
                Ok(Value::Integer(a))
            }
            SerialType::Integer16 => {
                let a = slice_to_array::<8>(&bytes[..serial_type.size()]);
                let a = i64::from_be_bytes(a);
                Ok(Value::Integer(a))
            }
            SerialType::Integer32 => {
                let a = slice_to_array::<8>(&bytes[..serial_type.size()]);
                let a = i64::from_be_bytes(a);
                Ok(Value::Integer(a))
            }
            SerialType::Integer48 => {
                let a = slice_to_array::<8>(&bytes[..serial_type.size()]);
                let a = i64::from_be_bytes(a);
                Ok(Value::Integer(a))
            }
            SerialType::Integer64 => {
                let a = slice_to_array::<8>(&bytes[..serial_type.size()]);
                let a = i64::from_be_bytes(a);
                Ok(Value::Integer(a))
            }
            SerialType::Text(s) => {
                let text = &bytes[..*s];
                let text = std::str::from_utf8(text)?;
                Ok(Value::Text(Cow::Borrowed(text)))
            }
            SerialType::Blob(s) => {
                let blob = &bytes[..*s];
                Ok(Value::Blob(Cow::Borrowed(blob)))
            }
            _ => Err("Unsupported serial type for value parsing")?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    use page::{BTreePageType, DatabaseHeader};

    #[test]
    fn test_varint() {
        struct TestCase<'a> {
            bytes: &'a [u8],
            expected_value: u64,
            expected_size: usize,
        }

        let test_cases = [
            TestCase {
                bytes: &[0x01],
                expected_value: 1,
                expected_size: 1,
            },
            TestCase {
                bytes: &[0x80, 0x01],
                expected_value: 1,
                expected_size: 2,
            },
            TestCase {
                bytes: &[0xff, 0x7f],
                expected_value: 16383,
                expected_size: 2,
            },
            TestCase {
                bytes: &[0xff, 0xff, 0x7f],
                expected_value: 2097151,
                expected_size: 3,
            },
            TestCase {
                bytes: &[0xff, 0xff, 0xff, 0x7f],
                expected_value: 268435455,
                expected_size: 4,
            },
            TestCase {
                bytes: &[0x8f, 0xff, 0xff, 0xff, 0x7f],
                expected_value: u32::MAX as u64,
                expected_size: 5,
            },
            TestCase {
                bytes: &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
                expected_value: u64::MAX,
                expected_size: 9,
            },
        ];

        for test_case in &test_cases {
            let varint = Varint::from_bytes(test_case.bytes).unwrap();
            assert_eq!(
                test_case.expected_value, varint.value,
                "expected value: {}, got: {}",
                test_case.expected_value, varint.value
            );
            assert_eq!(
                test_case.expected_size, varint.size,
                "expected size: {}, got: {}",
                test_case.expected_size, varint.size
            );
        }
    }

    #[test]
    fn test_serial_type() {
        let test_cases = [
            (0, SerialType::Null),
            (1, SerialType::Integer8),
            (2, SerialType::Integer16),
            (3, SerialType::Integer24),
            (4, SerialType::Integer32),
            (5, SerialType::Integer48),
            (6, SerialType::Integer64),
            (7, SerialType::Float64),
            (8, SerialType::Zero),
            (9, SerialType::One),
            (12, SerialType::Blob(0)),
            (14, SerialType::Blob(1)),
            (100, SerialType::Blob(44)),
            (13, SerialType::Text(0)),
            (15, SerialType::Text(1)),
            (101, SerialType::Text(44)),
        ];

        for (value, expected_serial_type) in &test_cases {
            let serial_type = SerialType::from_value(*value);
            assert_eq!(
                *expected_serial_type, serial_type,
                "expected: {:?}, got: {:?}",
                expected_serial_type, serial_type
            );
        }
    }

    #[test]
    fn test_cell_pointer() {
        struct TestCase {
            bytes: [u8; 2],
            expected_offset: u16,
        }
        let test_cases = [
            TestCase {
                bytes: [0x00, 0x00],
                expected_offset: 0,
            },
            TestCase {
                bytes: [0x00, 0x01],
                expected_offset: 1,
            },
            TestCase {
                bytes: [0x01, 0x00],
                expected_offset: 256,
            },
            TestCase {
                bytes: [0xFF, 0xFF],
                expected_offset: 65535,
            },
        ];

        for test_case in &test_cases {
            let cell_pointer = CellPointer::from_bytes(&test_case.bytes).unwrap();
            assert_eq!(
                test_case.expected_offset, cell_pointer.offset,
                "expected: {}, got: {}",
                test_case.expected_offset, cell_pointer.offset
            );
        }
    }

    #[test]
    fn test_parse_record_values() {
        struct TestCase {
            bytes: &'static [u8],
            serial_type: SerialType,
            expected: Value<'static>,
        }

        let test_cases = [
            TestCase {
                bytes: &[0x07],
                serial_type: SerialType::Integer8,
                expected: Value::Integer(7),
            },
            TestCase {
                bytes: &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
                serial_type: SerialType::Integer64,
                expected: Value::Integer(-1),
            },
            TestCase {
                bytes: &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
                serial_type: SerialType::Integer64,
                expected: Value::Integer(-1),
            },
            TestCase {
                bytes: b"hello",
                serial_type: SerialType::Text(5),
                expected: Value::Text("hello".into()),
            },
            TestCase {
                bytes: b"hello world universe cosmos galaxy solar system",
                serial_type: SerialType::Text(47),
                expected: Value::Text("hello world universe cosmos galaxy solar system".into()),
            },
        ];

        for test_case in &test_cases {
            let result = Record::parse_value(&test_case.serial_type, test_case.bytes);
            assert!(result.is_ok(), "Failed to parse value");

            let value = result.unwrap();
            assert_eq!(
                test_case.expected, value,
                "Expected {:?}, got: {:?}",
                test_case.expected, value
            );
        }
    }

    #[test]
    fn test_parse_record() {
        struct TestCase {
            bytes: &'static [u8],
            expected: Record<'static>,
        }

        let test_cases = [
            TestCase {
                bytes: &[0x02, 0x01, 0x08],
                expected: Record {
                    header_size: 2,
                    serial_types: vec![SerialType::Integer8],
                    values: vec![Value::Integer(8)],
                },
            },
            TestCase {
                bytes: &[0x02, 0x17, 0x68, 0x65, 0x6c, 0x6c, 0x6f],
                expected: Record {
                    header_size: 2,
                    serial_types: vec![SerialType::Text(5)],
                    values: vec![Value::Text("hello".into())],
                },
            },
            TestCase {
                bytes: &[0x03, 0x17, 0x01, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x7f],
                expected: Record {
                    header_size: 3,
                    serial_types: vec![SerialType::Text(5), SerialType::Integer8],
                    values: vec![Value::Text("hello".into()), Value::Integer(127)],
                },
            },
            TestCase {
                bytes: &[0x03, 0x01, 0x17, 0x7f, 0x68, 0x65, 0x6c, 0x6c, 0x6f],
                expected: Record {
                    header_size: 3,
                    serial_types: vec![SerialType::Integer8, SerialType::Text(5)],
                    values: vec![Value::Integer(127), Value::Text("hello".into())],
                },
            },
        ];

        for test_case in test_cases {
            let result = Record::parse(test_case.bytes);
            assert!(result.is_ok(), "Failed to parse record");

            let record = result.unwrap();
            assert_eq!(test_case.expected, record);
        }
    }

    #[test]
    fn test_parse_table_leaf_cell() {
        struct TestCase {
            bytes: &'static [u8],
            btree_page_type: BTreePageType,
            expected: TableLeafCell<'static>,
        }

        let test_cases = [
            TestCase {
                bytes: &[0x04, 0x01, 0x02, 0x01, 0x08],
                btree_page_type: BTreePageType::TableLeaf,
                expected: TableLeafCell::TableLeaf {
                    payload_size: 4,
                    rowid: 1,
                    payload: Record {
                        header_size: 2,
                        serial_types: vec![SerialType::Integer8],
                        values: vec![Value::Integer(8)],
                    },
                    overflow_page: None,
                },
            },
            TestCase {
                bytes: &[
                    0x0a, 0x01, 0x03, 0x01, 0x17, 0x08, 0x68, 0x65, 0x06c, 0x6c, 0x6f, 0x7f,
                ],
                btree_page_type: BTreePageType::TableLeaf,
                expected: TableLeafCell::TableLeaf {
                    payload_size: 10,
                    rowid: 1,
                    payload: Record {
                        header_size: 3,
                        serial_types: vec![SerialType::Integer8, SerialType::Text(5)],
                        values: vec![Value::Integer(8), Value::Text("hello".into())],
                    },
                    overflow_page: None,
                },
            },
            TestCase {
                bytes: &[
                    0x0a, 0x01, 0x03, 0x17, 0x01, 0x68, 0x65, 0x06c, 0x6c, 0x6f, 0x7f,
                ],
                btree_page_type: BTreePageType::TableLeaf,
                expected: TableLeafCell::TableLeaf {
                    payload_size: 10,
                    rowid: 1,
                    payload: Record {
                        header_size: 3,
                        serial_types: vec![SerialType::Text(5), SerialType::Integer8],
                        values: vec![Value::Text("hello".into()), Value::Integer(127)],
                    },
                    overflow_page: None,
                },
            },
        ];

        for test_case in test_cases {
            let result = TableLeafCell::parse(test_case.btree_page_type, test_case.bytes);
            assert!(result.is_ok(), "Failed to parse record");

            let table_leaf_cell = result.unwrap();
            assert_eq!(test_case.expected, table_leaf_cell);
        }
    }
}
