#![allow(unused)]

use std::error::Error;

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
pub enum FreeListPageType {
    /// Freelist trunk page - contains pointers to freelist leaf pages
    Trunk,
    /// Freelist leaf page - contains actual freed pages
    Leaf,
}

impl FreeListPageType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x02 => Some(FreeListPageType::Trunk),
            0x00 => Some(FreeListPageType::Leaf),
            _ => None,
        }
    }
}

pub struct TableLeafPage {
    pub header: BTreePageHeader,
    pub payload: Vec<u8>,
}

pub enum SqlitePageType {
    BTree(BTreePageType),
    FreeList(FreeListPageType),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    #[test]
    fn test_read_db_header() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test.db");
        let conn = test_utils::init_db(&file_path);
        let bytes = std::fs::read(&file_path).unwrap();
        let db_header = DatabaseHeader::from(&bytes).unwrap();
        assert_eq!("SQLite format 3", db_header.magic_header);
        assert_eq!(4096, db_header.page_size); // Standard page size
        assert_eq!(1, db_header.write_version);
        assert_eq!(1, db_header.read_version);
        assert_eq!(0, db_header.page_reserved_space);
        assert_eq!(64, db_header.max_payload_fraction);
        assert_eq!(32, db_header.min_payload_fraction);
        assert_eq!(32, db_header.leaf_payload_fraction);
        assert_eq!(db_header.change_counter, 5); // Should be at least 1 after our operations
        assert_eq!(db_header.database_size_pages, 3); // At least 3 pages (header, table, index)
        assert_eq!(0, db_header.first_freelist_trunk_page); // No freelist pages yet
        assert_eq!(0, db_header.total_freelist_pages); // No freelist pages yet
        assert!(db_header.schema_cookie > 0); // Should be incremented for each schema change
        assert_eq!(4, db_header.schema_format); // Current schema format
        assert_eq!(0, db_header.default_cache_size); // Default cache size
        assert_eq!(0, db_header.largest_root_btree_page); // Autovacuum not in use
        assert_eq!(1, db_header.text_encoding); // 1 = UTF-8
        assert_eq!(0, db_header.user_version); // Default user version
        assert_eq!(0, db_header.incremental_vacuum); // No incremental vacuum
        assert_eq!(0, db_header.application_id); // This might not be worth testing
        assert_ne!(0, db_header.version_valid); // This might not be worth testing
        assert_eq!(3, db_header.sqlite_version / 1_000_000); // version string typically looks like: "3049001"
    }

    #[test]
    fn test_btree_page_header() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_btree_page_header.db");
        let conn = test_utils::init_db(&file_path);
        let bytes = std::fs::read(&file_path).unwrap();
        let btree_page_header = BTreePageHeader::from_bytes(&bytes[100..108]).unwrap();

        assert_eq!(btree_page_header.page_type, BTreePageType::TableLeaf);
        assert_eq!(btree_page_header.first_freeblock, 0);
        assert_eq!(btree_page_header.cell_count, 2);
        assert_eq!(btree_page_header.cell_content_offset, 3919);
        assert_eq!(btree_page_header.fragmented_free_bytes, 0);
        assert_eq!(btree_page_header.rightmost_pointer, None);
    }
}
