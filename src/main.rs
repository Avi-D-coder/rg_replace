use std::io::{self, Write};
use std::ops::Range;
use std::process::Command;

extern crate clap;
use clap::{App, Arg};
use std::env;

extern crate grep_json_deserialize as deserialize;
use deserialize::{ArbitraryData::*, Type::*, *};

fn main() {
    let matches = App::new("rg_replace")
       .version("0.1.0")
       .about("Just ripgrep, but --replace can write to files or generate a patch in unified diff format!")
       .arg(Arg::with_name("replace")
            .short("R")
            .long("replace")
            .takes_value(true)
            .help("Takes a string to replace matches with. When the flag or option are not present rgr will just pass it's arguments to ripgrep."))
       .arg(Arg::with_name("diff").long("diff").takes_value(true).help("Takes file to write unified diff to. If left empty will write diff to stdout"))
       .arg(Arg::with_name("iterative").long("iterative").takes_value(false).help("Steps through matches confirming before writing changes. On by default in absence of --diff. Maybe be combined with diff to interactively generate diff.\n\n
Non Interactive direct search and replace is DANGEROUS,
since ripgrep is RECURSIVE use: `yes | rgr <arguments>` AT YOUR OWN RISK"))
       .get_matches();

    let flags = (
        matches.value_of("replace"),
        matches.values_of("diff"),
        matches.values_of("iterative"),
    );

    match flags {
        (Some(replace), Some(diff_out), Some(_)) => unimplemented!(),
        (Some(replace), Some(diff), None) => unimplemented!(),
        (Some(replace), _, _) => unimplemented!(),
        (None, None, None) => unimplemented!(),
        (None, _, _) => panic!("You can't use --iterative or --diff without --replace"),
    };
}

fn handle_args(args: Vec<String>, remove_args: Vec<(&str, bool)>) -> Vec<String> {
    let mut args = remove_options(args, remove_args);

    let mut is_present = args.iter().fold(
        [
            ("--json", false),
            ("--line_number", false),
            ("--context", false),
        ],
        |[(json_text, json_present),
        (line_number_text,line_number_present),
        (context_text,context_present)],
        arg| {
            [
                (json_text, json_present || arg == json_text),
                (line_number_text, line_number_present || arg == line_number_text),
                (context_text, context_present || arg == context_text),
            ]
        },
    );

    is_present[2].0 = "--context=3";

    for (arg, needed) in is_present.iter() {
        if *needed {
            args.push(arg.to_string())
        }
    }
    args
}

fn generate_diff_interactively() {
    let rgr_args: Vec<String> = env::args().skip(1).collect();
    let pass_args = handle_args(
        rgr_args,
        vec![
            ("--replace", true),
            ("-R", true),
            ("--diff", true),
            ("--iterative", false),
        ],
    );

    let files = rg_call_output(pass_args);

    files.into_iter().map(|file| {
        println!("{}", file.path.lossy_utf8());
        let stdout = io::stdout();

        // make indent for line numbers
        let line_numbers = file.lines.iter().map(|l| l.line_number().to_string());
        let indent = line_numbers.map(|s| s.len()).max().unwrap();

        let mut stdout = stdout.lock();

        for line in file.lines.iter() {
            for i in 0..indent {
                line.line_number().to_string().chars().nth(i);
            }
        }
    });
}

#[derive(Debug)]
enum Line {
    Matched {
        line_number: usize,
        absolute_offset: isize,
        lines: ArbitraryData,
        sub_matches: Vec<Range<usize>>,
    },
    // TODO spec says context can have submatch
    Context {
        line_number: usize,
        text: String,
    },
}

impl Line {
    fn line_number(&self) -> usize {
        match self {
            Line::Matched { line_number, .. } => *line_number,
            Line::Context { line_number, .. } => *line_number,
        }
    }
}

#[derive(Debug)]
struct FileGroup {
    path: ArbitraryData,
    // matches: Vec<Match>,
    lines: Vec<Line>,
}

fn rg_call_output(args: Vec<String>) -> Vec<FileGroup> {
    let out = Command::new("rg")
        .args(args)
        .output()
        .expect("rg not found. Install ripgrep");

    let out = unsafe { String::from_utf8_unchecked(out.stdout) };

    let mut files = vec![];
    let mut file_path = None;
    let mut file_lines = vec![];
    for line in out.lines() {
        let line: Type = serde_json::from_str(&line).unwrap();
        match line {
            Begin { path } => file_path = Some(path),
            Match {
                lines,
                line_number,
                absolute_offset,
                submatches,
                ..
            } => file_lines.push(Line::Matched {
                // We always pass `--line-number`.
                line_number: line_number.unwrap(),
                absolute_offset,
                lines,
                sub_matches: submatches.into_iter().map(|m| m.start..m.end).collect(),
            }),
            Context {
                line_number, lines, ..
            } => file_lines.push(Line::Context {
                line_number: line_number.unwrap(),
                text: lines.lossy_utf8(),
            }),
            End { .. } => {
                files.push(FileGroup {
                    // a Begin will always set file_path before we unwrap it.
                    path: file_path.unwrap(),
                    lines: file_lines,
                });
                file_path = None;
                file_lines = vec![];
            }
            Summary { .. } => {}
        };
    }
    files
}

fn remove_options(args: Vec<String>, options: Vec<(&str, bool)>) -> Vec<String> {
    let mut skip: usize = 0;

    args.into_iter()
        .filter(|arg| {
            fn skip_n(n: usize, skip: &mut usize) -> bool {
                *skip = n - 1;
                false
            }

            options.iter().any(|(option, has_value)| {
                if skip == 0 {
                    match (arg.starts_with(option), has_value) {
                        (true, true) => {
                            if arg.starts_with(&format!("{}=", option)) {
                                false
                            } else {
                                skip_n(2, &mut skip)
                            }
                        }
                        (true, false) => false,
                        (false, _) => true,
                    }
                } else {
                    skip_n(1, &mut skip)
                }
                // match (
                //     arg.starts_with(option),
                //     arg.starts_with(&format!("{}=", option)),
                //     skip,
                //     has_value,
                // ) {
                //     (true, false, _, true) => skip_n(2, &mut skip),
                //     (true, _, _, _) => false,
                //     (_, _, 0, _) => true,
                //     (_, _, n, _) => skip_n(n, &mut skip),
                // }
            })
        })
        .collect()
}

#[allow(dead_code)]
fn to_strings(strs: Vec<&str>) -> Vec<String> {
    strs.into_iter().map(|s| s.to_owned()).collect()
}
#[test]
fn remove_option_and_value_from_start() {
    let replace = ("--replace", true);
    assert_eq!(
        vec!["good"],
        remove_options(to_strings(vec![replace.0, "arg", "good"]), vec![replace])
    );
}

#[test]
fn remove_option_and_value_from_end() {
    let replace = ("--replace", true);
    assert_eq!(
        vec!["good"],
        remove_options(to_strings(vec!["good", replace.0, "arg"]), vec![replace])
    );
}

#[test]
fn remove_option_from_start() {
    let diff = ("--diff", false);
    assert_eq!(
        vec!["not-arg"],
        remove_options(to_strings(vec![diff.0, "not-arg"]), vec![diff])
    );
}

#[test]
fn remove_option_from_middle() {
    let diff = ("--diff", false);
    assert_eq!(
        vec!["good", "not-arg"],
        remove_options(to_strings(vec!["good", diff.0, "not-arg"]), vec![diff])
    );
}

#[test]
fn remove_option_from_end() {
    let diff = ("--diff", true);
    assert_eq!(
        vec!["good"],
        remove_options(to_strings(vec!["good", diff.0]), vec![diff])
    );
}
