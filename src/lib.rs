use std::convert::TryFrom;
use std::default::Default;
use std::{error, fmt, fs, path, process};

use colored::*;
use regex::Regex;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Cli {
    /// path to the file to split in half
    #[structopt(parse(from_os_str))]
    pub infile: path::PathBuf,
}

#[derive(Default)]
struct SplitFile {
    path: path::PathBuf,
    bytes: Vec<u8>,
    size: usize,
    middle: usize,
    left_sig: Signature,
    right_sig: Signature,
}

enum SplitHalf {
    Left,
    Right,
}

#[derive(Default)]
struct Signature {
    start_offset: u32,
    end_offset: u32,
}

impl Signature {
    const LEN: usize = 12;
    const TAG: [u8; 4] = [0x65, 0x70, 0x69, 0x3a]; // epi:

    /// Simple helper to return a String showing the start and end offsets as a range
    fn range_str(&self) -> String {
        format!("{}-{}", self.start_offset, self.end_offset)
    }

    /// Returns 12 byte representation of a Signature; TAG + START_OFFSET + END_OFFSET
    fn as_bytes(&self) -> [u8; 12] {
        let mut ret: [u8; 12] = [0; 12];

        let concatenated = &[
            Signature::TAG,
            u32::to_le_bytes(self.start_offset),
            u32::to_le_bytes(self.end_offset),
        ]
        .concat();

        for (i, ch) in concatenated.iter().enumerate() {
            ret[i] = *ch;
        }

        ret
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\n[-] {{\n[-]\t{}: {}\n[-]\t{}:   {}\n[-] }}",
            "start".bright_yellow(),
            self.start_offset,
            "end".bright_yellow(),
            self.end_offset
        )
    }
}

impl SplitFile {
    fn from_path(path: &path::PathBuf) -> SplitFile {
        let bytes = match fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[!] Could not open {}: {}", path.as_path().display(), e);
                process::exit(5);
            }
        };

        let path = path.to_path_buf();
        let size = bytes.len();
        let middle = size / 2;
        let left_sig = Signature::default();
        let right_sig = Signature::default();

        SplitFile {
            path,
            bytes,
            size,
            middle,
            left_sig,
            right_sig,
        }
    }

    fn data(&self, half: SplitHalf) -> &[u8] {
        match half {
            SplitHalf::Left => &self.bytes[0..self.middle],
            SplitHalf::Right => &self.bytes[self.middle..],
        }
    }

    fn left_sig_mut(&mut self) -> &mut Signature {
        &mut self.left_sig
    }

    fn right_sig_mut(&mut self) -> &mut Signature {
        &mut self.right_sig
    }

    fn signature(&self, half: SplitHalf) -> &Signature {
        match half {
            SplitHalf::Left => &self.left_sig,
            SplitHalf::Right => &self.right_sig,
        }
    }

    fn set_signature(&mut self, start: u32, end: u32, half: SplitHalf) {
        let to_set = Signature {
            start_offset: start,
            end_offset: end,
        };

        match half {
            SplitHalf::Left => {
                *self.left_sig_mut() = to_set;
            }
            SplitHalf::Right => {
                *self.right_sig_mut() = to_set;
            }
        }
    }

    fn read_signature(&mut self) -> Option<Signature> {
        match &self.bytes.len() > &Signature::LEN && &self.bytes[0..4] == &Signature::TAG {
            true => {
                // existing signature
                let start_array = match <[u8; 4]>::try_from(&self.bytes[4..8]) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("[!] Could not read starting offset: {}", e);
                        return None;
                    }
                };

                let end_array = match <[u8; 4]>::try_from(&self.bytes[8..12]) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("[!] Could not read ending offset: {}", e);
                        return None;
                    }
                };

                let start_offset = u32::from_le_bytes(start_array);
                let end_offset = u32::from_le_bytes(end_array);

                if start_offset > end_offset {
                    println!("[!] Starting offset is larger than ending offset; exiting.");
                    process::exit(2);
                }

                Some(Signature {
                    start_offset,
                    end_offset,
                })
            }
            false => {
                // new file, should be 0 -> middle, and middle -> end
                &self.set_signature(0, self.middle as u32, SplitHalf::Left);
                &self.set_signature(
                    self.data(SplitHalf::Right).len() as u32,
                    self.size as u32,
                    SplitHalf::Right,
                );
                None
            }
        }
    }

    fn filename(&self, half: SplitHalf) -> String {
        let sig: &Signature;

        match half {
            SplitHalf::Left => {
                sig = &self.signature(SplitHalf::Left);
            }
            SplitHalf::Right => {
                sig = &self.signature(SplitHalf::Right);
            }
        };

        let stem = match self.path.file_stem() {
            Some(data) => data,
            None => {
                eprintln!("[!] Error getting file name; exiting.");
                process::exit(3);
            }
        };

        let re = Regex::new(r"^[0-9]+-[0-9]+-").unwrap();

        match stem.to_str() {
            Some(d) => {
                format!("{}-{}.bin", sig.range_str(), re.replace(d, ""))
            }
            None => {
                format!("{}-{:?}.bin", sig.range_str(), stem)
            }
        }


    }

    fn save(&self) {
        let leftname = self.filename(SplitHalf::Left);
        println!("{} {}", "[=]".bright_blue(), "Saving first half");

        match fs::write(
            &leftname,
            [&self.left_sig.as_bytes(), self.data(SplitHalf::Left)].concat(),
        ) {
            Ok(_d) => println!(
                "{} {} {}",
                "[+]".bright_green(),
                "Successfully saved",
                leftname
            ),
            Err(e) => println!(
                "{} {} {}: {}",
                "[!]".bright_red(),
                "Could not save",
                leftname,
                e
            ),
        }

        let rightname = self.filename(SplitHalf::Right);
        println!("{} {}", "[=]".bright_blue(), "Saving second half");

        match fs::write(
            &rightname,
            [&self.right_sig.as_bytes(), self.data(SplitHalf::Right)].concat(),
        ) {
            Ok(_d) => println!(
                "{} {} {}",
                "[+]".bright_green(),
                "Successfully saved",
                rightname
            ),
            Err(e) => println!(
                "{} {} {}: {}",
                "[!]".bright_red(),
                "Could not save",
                leftname,
                e
            ),
        }
    }
}

pub fn run(args: &Cli) -> Result<(), Box<dyn error::Error>> {
    let mut split = SplitFile::from_path(&args.infile);

    let left = split.data(SplitHalf::Left);
    let right = split.data(SplitHalf::Right);

    if left.len() + right.len() != split.size {
        eprintln!("{} {}", "[!]".bright_red(), "Incomplete file read");
        process::exit(1);
    }

    match split.read_signature() {
        Some(sig) => {
            println!(
                "{} {} {}",
                "[=]".bright_blue(),
                "Found a previously split file with signature:",
                sig,
            );

            let halfway = (sig.end_offset - sig.start_offset) / 2;
            let middle = sig.start_offset + halfway;

            split.set_signature(sig.start_offset, middle, SplitHalf::Left);
            split.set_signature(middle, sig.end_offset, SplitHalf::Right);
        }
        None => {
            println!(
                "{} {}",
                "[=]".bright_blue(),
                "No signature found; commencing the first split of a pristine file!"
            );
        }
    };

    split.save();

    Ok(())
}
