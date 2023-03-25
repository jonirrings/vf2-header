use clap::{arg, ArgAction, Parser};
use clap_num::maybe_hex;
use crc::{Crc, CRC_32_ISO_HDLC};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use log::error;

// const DEF_VER: u32 = 0x01010101;
// const DEF_BACKUP: u32 = 0x200000;
const CRC_FAILED: u32 = 0x5A5A5A5A;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, disable_version_flag = true, arg_required_else_help = true)]
struct Args {
    #[arg(short, long = "creat-splhdr", help = "creat spl hdr", action = ArgAction::SetTrue)]
    c: bool,
    #[arg(short, long = "fix-imghdr", help = "fixed img hdr for emmc boot", action = ArgAction::SetTrue)]
    i: bool,
    #[arg(short, long = "spl-bak-addr", help = "set backup SPL addr", value_parser = maybe_hex::< u32 >, default_value = "0x200000")]
    a: u32,
    #[arg(short, long = "version", help = "set version", value_parser = maybe_hex::< u32 >, default_value = "0x01010101")]
    v: u32,
    #[arg(short, long = "file", help = "input file name")]
    f: String,
}

#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
struct UBootSPLHeader {
    // offset of spl header: 64+256+256 = 0x240
    sofs: u32,
    // SBL_BAK_OFFSET: Offset of backup SBL from Flash info start (from input_sbl_normal.cfg)
    bofs: u32,
    #[serde(with = "serde_arrays")]
    zro2: [u8; 636],
    // version: shall be 0x01010101 (from https://doc-en.rvspace.org/VisionFive2/SWTRM/VisionFive2_SW_TRM/create_spl.html)
    vers: u32,
    // u-boot-spl.bin size in bytes
    fsiz: u32,
    // Offset from HDR to SPL_IMAGE, 0x400 (00 04 00 00) currently
    res1: u32,
    // CRC32 of u-boot-spl.bin
    crcs: u32,
    #[serde(with = "serde_arrays")]
    zro3: [u8; 364],
}


impl UBootSPLHeader {
    fn new() -> Self {
        Self {
            sofs: 0x240u32.to_le(),
            bofs: 0,
            zro2: [0; 636],
            vers: 0,
            fsiz: 0,
            res1: 0x400u32.to_le(),
            crcs: 0,
            zro3: [0; 364],
        }
    }
}

struct HeaderConf {
    name: String,
    vers: u32,
    bofs: u32,
    create_hdr: bool,
    fix_img_hdr: bool,
}

impl From<Args> for HeaderConf {
    fn from(args: Args) -> Self {
        Self {
            name: args.f,
            bofs: args.a.to_le(),
            vers: args.v.to_le(),
            create_hdr: args.c,
            fix_img_hdr: args.i,
        }
    }
}

fn write_spl_hdr(conf: &HeaderConf) {
    let mut spl_hdr: UBootSPLHeader = UBootSPLHeader::new();
    spl_hdr.bofs = conf.bofs;
    spl_hdr.vers = conf.vers;
    println!(
        "spl_hdr.sofs: 0x{:x}, spl_hdr.bofs: 0x{:x}, spl_hdr.vers: 0x{:x} name:{}",
        spl_hdr.sofs,
        spl_hdr.bofs,
        spl_hdr.vers,
        conf.name.clone()
    );
    let mut file = File::open(conf.name.clone()).unwrap(); //fixme: error case handle
    let metadata = file.metadata().unwrap(); //fixme: error case handle
    let max_size = (181072 - size_of::<UBootSPLHeader>() + 1) as u32;
    let f_size = metadata.len() as u32;
    if f_size > max_size {
        panic!("File too large! Please rebuild your SPL with -Os. Maximum allowed size is {} bytes.", max_size);
    }
    spl_hdr.fsiz = f_size.to_le();
    let mut contents = Vec::new();
    let _res = file.read_to_end(&mut contents); //fixme: error case handle
    let mut file = File::create(format!("{}.normal.out", conf.name.clone())).unwrap(); //fixme: error case handle
    let crc32 = Crc::<u32>::new(&CRC_32_ISO_HDLC);
    let mut digest = crc32.digest();
    digest.update(contents.as_slice());
    spl_hdr.crcs = digest.finalize().to_le();
    let v = bincode::serialize(&spl_hdr).unwrap(); //fixme: error case handle
    let _res = file.write(v.as_slice()); //fixme: error case handle
    let _res = file.write(contents.as_slice()); //fixme: error case handle
}

/// When starting with emmc, bootrom will read 0x0 instead of partition 0. (Known issues).
/// Read GPT PMBR+Header, then write the backup address at 0x4, and write the wrong CRC
/// check value at 0x290, so that bootrom CRC check fails and jump to the backup address
/// to load the real spl.
fn write_img_hdr(conf: &HeaderConf) {
    let mut file = File::options()
        .read(true)
        .write(true)
        .open(conf.name.clone())
        .unwrap(); //fixme: error case handle
    let mut contents = vec![0u8; size_of::<UBootSPLHeader>()];
    let _res = file.read(&mut contents); //fixme: error case handle
    let mut img_hdr: UBootSPLHeader = bincode::deserialize(contents.as_slice()).unwrap();//fixme: error case handle
    if conf.bofs != 0 {
        img_hdr.bofs = conf.bofs;
    }
    img_hdr.crcs = CRC_FAILED.to_le();
    let _res = file.seek(SeekFrom::Start(0)); //fixme: error case handle
    let v = bincode::serialize(&img_hdr).unwrap(); //fixme: error case handle
    let _res = file.write(v.as_slice()); //fixme: error case handle
    println!("IMG {} fixed hdr successfully.", conf.name.clone());
}

fn main() {
    let args = Args::parse();
    env_logger::init();
    let hdr_conf: HeaderConf = args.into();
    if hdr_conf.create_hdr {
        write_spl_hdr(&hdr_conf);
        return;
    }
    if hdr_conf.fix_img_hdr {
        write_img_hdr(&hdr_conf);
    }
}
