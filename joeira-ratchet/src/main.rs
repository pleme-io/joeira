use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use joeira_ratchet::{BankMode, Baseline};

const USAGE: &str = "usage:
  ratchet judge --baseline <file> [--measurements <file>|-]
  ratchet bank --baseline <file> [--measurements <file>|-] [--record]
  ratchet render [--baseline <file>|-]";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Stdin,
    File(PathBuf),
}

impl Source {
    fn of(arg: &str) -> Source {
        if arg == "-" {
            Source::Stdin
        } else {
            Source::File(PathBuf::from(arg))
        }
    }

    fn read(&self) -> Result<String, Fault> {
        match self {
            Source::Stdin => {
                let mut text = String::new();
                io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|source| Fault::Read {
                        path: "-".into(),
                        source,
                    })?;
                Ok(text)
            }
            Source::File(path) => std::fs::read_to_string(path).map_err(|source| Fault::Read {
                path: path.display().to_string(),
                source,
            }),
        }
    }
}

#[derive(Debug)]
enum Command {
    Judge {
        baseline: Source,
        measurements: Source,
    },
    Bank {
        baseline: PathBuf,
        measurements: Source,
        mode: BankMode,
    },
    Render {
        baseline: Source,
    },
}

#[derive(Debug)]
enum Fault {
    Usage(String),
    Read { path: String, source: io::Error },
    Write { path: String, source: io::Error },
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Usage(why) => write!(f, "ratchet: {why}\n{USAGE}"),
            Fault::Read { path, source } => write!(f, "ratchet: cannot read {path}: {source}"),
            Fault::Write { path, source } => write!(f, "ratchet: cannot write {path}: {source}"),
        }
    }
}

fn parse(args: &[String]) -> Result<Command, Fault> {
    let Some((verb, rest)) = args.split_first() else {
        return Err(Fault::Usage("no command".into()));
    };
    let mut baseline = None;
    let mut measurements = None;
    let mut record = false;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--baseline" | "--measurements" => {
                let value = it
                    .next()
                    .ok_or_else(|| Fault::Usage(format!("{flag} needs a value")))?;
                let slot = if flag == "--baseline" {
                    &mut baseline
                } else {
                    &mut measurements
                };
                if slot.replace(value.clone()).is_some() {
                    return Err(Fault::Usage(format!("{flag} given twice")));
                }
            }
            "--record" => record = true,
            other => return Err(Fault::Usage(format!("unknown argument {other}"))),
        }
    }
    let refuse = |what: &str| Err(Fault::Usage(format!("{what} does not apply to {verb}")));
    match verb.as_str() {
        "judge" => {
            if record {
                return refuse("--record");
            }
            Ok(Command::Judge {
                baseline: Source::of(
                    &baseline.ok_or_else(|| Fault::Usage("judge needs --baseline".into()))?,
                ),
                measurements: Source::of(measurements.as_deref().unwrap_or("-")),
            })
        }
        "bank" => {
            let baseline =
                baseline.ok_or_else(|| Fault::Usage("bank needs --baseline <file>".into()))?;
            if baseline == "-" {
                return Err(Fault::Usage("bank rewrites a file, not stdin".into()));
            }
            Ok(Command::Bank {
                baseline: PathBuf::from(baseline),
                measurements: Source::of(measurements.as_deref().unwrap_or("-")),
                mode: if record {
                    BankMode::Record
                } else {
                    BankMode::Shrink
                },
            })
        }
        "render" => {
            if record {
                return refuse("--record");
            }
            if measurements.is_some() {
                return refuse("--measurements");
            }
            Ok(Command::Render {
                baseline: Source::of(baseline.as_deref().unwrap_or("-")),
            })
        }
        other => Err(Fault::Usage(format!("unknown command {other}"))),
    }
}

fn write_in_place(path: &Path, text: &str) -> Result<(), Fault> {
    let fault = |source| Fault::Write {
        path: path.display().to_string(),
        source,
    };
    let mut staged = path.as_os_str().to_owned();
    staged.push(".ratchet-tmp");
    let staged = PathBuf::from(staged);
    std::fs::write(&staged, text).map_err(fault)?;
    std::fs::rename(&staged, path).map_err(fault)
}

fn run(command: Command) -> Result<bool, Fault> {
    match command {
        Command::Judge {
            baseline,
            measurements,
        } => {
            let recorded = Baseline::parse(&baseline.read()?);
            let measured = Baseline::parse(&measurements.read()?);
            let summary = recorded.summarize(&measured);
            print!("{summary}");
            Ok(!summary.is_red())
        }
        Command::Bank {
            baseline,
            measurements,
            mode,
        } => {
            let recorded = Baseline::parse(&Source::File(baseline.clone()).read()?);
            let measured = Baseline::parse(&measurements.read()?);
            let banked = recorded.bank(&measured, mode);
            write_in_place(&baseline, &banked.render())?;
            println!(
                "banked {}: {} rows before, {} after",
                baseline.display(),
                recorded.len(),
                banked.len()
            );
            Ok(true)
        }
        Command::Render { baseline } => {
            print!("{}", Baseline::parse(&baseline.read()?).render());
            Ok(true)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args).and_then(run) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(fault) => {
            eprintln!("{fault}");
            ExitCode::from(2)
        }
    }
}
