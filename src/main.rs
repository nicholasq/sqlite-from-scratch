use std::{
    env,
    error::Error,
    fs::File,
    io::{BufReader, Read},
};

use sqlite_from_scratch::page;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <database-file>", args[0]);
        std::process::exit(1);
    }

    let mut buf_reader = BufReader::new(File::open(&args[1])?);
    let mut buffer = [0u8; 100];
    buf_reader.read_exact(&mut buffer)?;

    let db_header = page::DatabaseHeader::from(&buffer)?;
    let buffer = &mut vec![0u8; db_header.page_size as usize];
    let file = File::open(&args[1])?;
    let mut buf_reader = BufReader::new(file);

    println!("{:#?}", db_header);

    for page_num in 1..db_header.database_size_pages + 1 {
        buf_reader.read_exact(buffer)?;
        let page_header = if page_num == 1 {
            page::BTreePageHeader::from_bytes(&buffer[100..112])
        } else {
            page::BTreePageHeader::from_bytes(&buffer[0..12])
        };
        println!("{:#?}", page_header);
    }

    Ok(())
}
