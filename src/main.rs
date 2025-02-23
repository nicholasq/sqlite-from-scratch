use std::{
    env,
    error::Error,
    fs::File,
    io::{BufReader, Read},
};

#[derive(Debug)]
pub struct BTreePageHeader {
    /// Type of B-tree page (values 2, 5, 10, or 13)
    pub page_type: BTreePageType,
    /// Offset to first freeblock (0 if none)
    pub first_freeblock: u16,
    /// Number of cells on page
    pub cell_count: u16,
    /// Start of cell content area
    pub cell_content_offset: u16,
    /// Number of fragmented free bytes
    pub fragmented_free_bytes: u8,
    /// Right-most pointer (only exists on interior pages)
    pub rightmost_pointer: Option<u32>,
}

impl BTreePageHeader {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let rightmost_pointer = if bytes[0] == 0x02 || bytes[0] == 0x05 {
            // Interior pages have rightmost pointer
            Some(u32::from_be_bytes(bytes[8..12].try_into().unwrap()))
        } else {
            None
        };

        Ok(BTreePageHeader {
            page_type: BTreePageType::from_byte(bytes[0])?,
            first_freeblock: u16::from_be_bytes(bytes[1..3].try_into()?),
            cell_count: u16::from_be_bytes(bytes[3..5].try_into()?),
            cell_content_offset: u16::from_be_bytes(bytes[5..7].try_into()?),
            fragmented_free_bytes: bytes[7],
            rightmost_pointer,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum BTreePageType {
    /// Interior page of a table B-tree
    TableInterior,
    /// Leaf page of a table B-tree containing actual table content
    TableLeaf,
    /// Interior page of an index B-tree
    IndexInterior,
    /// Leaf page of an index B-tree containing indexed values
    IndexLeaf,
}

impl BTreePageType {
    pub fn from_byte(byte: u8) -> Result<Self, Box<dyn Error>> {
        match byte {
            0x05 => Ok(BTreePageType::TableInterior),
            0x0D => Ok(BTreePageType::TableLeaf),
            0x02 => Ok(BTreePageType::IndexInterior),
            0x0A => Ok(BTreePageType::IndexLeaf),
            _ => Err("Invalid B-tree page type".into()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum FreeListPage {
    /// Freelist trunk page - contains pointers to freelist leaf pages
    Trunk,
    /// Freelist leaf page - contains actual freed pages
    Leaf,
}

impl FreeListPage {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x02 => Some(FreeListPage::Trunk),
            0x00 => Some(FreeListPage::Leaf),
            _ => None,
        }
    }
}

pub enum DbPage {
    BTree(BTreePageType),
    FreeList(FreeListPage),
    PayloadOverflowPage,
    PointerMapPage,
    LockBytePage,
}

#[derive(Debug)]
pub struct DatabaseHeader {
    pub magic_header: String,
    pub page_size: u16,
    pub write_version: u8,
    pub read_version: u8,
    pub page_reserved_space: u8,
    pub max_payload_fraction: u8,
    pub min_payload_fraction: u8,
    pub leaf_payload_fraction: u8,
    pub change_counter: u32,
    pub database_size_pages: u32,
    pub first_freelist_trunk_page: u32,
    pub total_freelist_pages: u32,
    pub schema_cookie: u32,
    pub schema_format: u32,
    pub default_cache_size: u32,
    pub largest_root_btree_page: u32,
    pub text_encoding: u32,
    pub user_version: u32,
    pub incremental_vacuum: u32,
    pub application_id: u32,
    pub reserved: [u8; 20],
    pub version_valid: u32,
    pub sqlite_version: u32,
}

impl DatabaseHeader {
    pub fn from(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let magic_str = std::str::from_utf8(&bytes[0..15])?.to_string();

        let mut reserved = [0u8; 20];
        reserved.copy_from_slice(&bytes[72..92]);

        Ok(DatabaseHeader {
            magic_header: magic_str,
            page_size: u16::from_be_bytes(bytes[16..18].try_into()?),
            write_version: bytes[18],
            read_version: bytes[19],
            page_reserved_space: bytes[20],
            max_payload_fraction: bytes[21],
            min_payload_fraction: bytes[22],
            leaf_payload_fraction: bytes[23],
            change_counter: u32::from_be_bytes(bytes[24..28].try_into()?),
            database_size_pages: u32::from_be_bytes(bytes[28..32].try_into()?),
            first_freelist_trunk_page: u32::from_be_bytes(bytes[32..36].try_into()?),
            total_freelist_pages: u32::from_be_bytes(bytes[36..40].try_into()?),
            schema_cookie: u32::from_be_bytes(bytes[40..44].try_into()?),
            schema_format: u32::from_be_bytes(bytes[44..48].try_into()?),
            default_cache_size: u32::from_be_bytes(bytes[48..52].try_into()?),
            largest_root_btree_page: u32::from_be_bytes(bytes[52..56].try_into()?),
            text_encoding: u32::from_be_bytes(bytes[56..60].try_into()?),
            user_version: u32::from_be_bytes(bytes[60..64].try_into()?),
            incremental_vacuum: u32::from_be_bytes(bytes[64..68].try_into()?),
            application_id: u32::from_be_bytes(bytes[68..72].try_into()?),
            reserved,
            version_valid: u32::from_be_bytes(bytes[92..96].try_into()?),
            sqlite_version: u32::from_be_bytes(bytes[96..100].try_into()?),
        })
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <database-file>", args[0]);
        std::process::exit(1);
    }

    let mut file = File::open(&args[1]).unwrap();
    let mut buffer = [0u8; 100];
    file.read_exact(&mut buffer).unwrap();

    let db_header = DatabaseHeader::from(&buffer)?;
    let buffer = &mut vec![0u8; db_header.page_size as usize];
    let file = File::open(&args[1]).unwrap();
    let mut buf_reader = BufReader::new(file);

    println!("{:#?}", db_header);

    for page_num in 1..db_header.database_size_pages + 1 {
        buf_reader.read_exact(buffer).unwrap();
        let page_header = if page_num == 1 {
            BTreePageHeader::from_bytes(&buffer[100..112])
        } else {
            BTreePageHeader::from_bytes(&buffer[0..12])
        };
        println!("{:#?}", page_header);
    }

    Ok(())
}
