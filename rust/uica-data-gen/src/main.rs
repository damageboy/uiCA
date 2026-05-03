use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let xml_path = args.next();
    let out_dir = args.next();

    if xml_path.is_none() || out_dir.is_none() || args.next().is_some() {
        eprintln!("usage: {:?} <instructions.xml> <output-dir>", program);
        std::process::exit(1);
    }

    let xml_path = xml_path.unwrap();
    let out_dir = out_dir.unwrap();
    uica_data_gen::convert_xml_to_pack_dir(Path::new(&xml_path), Path::new(&out_dir))?;
    Ok(())
}
