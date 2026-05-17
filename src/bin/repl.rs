// ABOUTME: Interactive REPL for the dual-mode reduction engine.
// ABOUTME: History line-editing via rustyline, toggleable through config.

use nat_calc::config::{self, Config};
use nat_calc::{Environment, eval};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::io::{self, BufRead, Write};

fn main() {
    let cfg = config::load();
    let mut env = Environment::new();

    println!("nat-calc - dual-mode (eager/lazy) reduction engine");
    println!("Type expressions. `simplify(...)`, `expand(...)`, `derive(x, ...)`.");
    if cfg.history {
        println!("Up/Down arrows recall history. Ctrl-D to exit.\n");
        run_with_history(&cfg, &mut env);
    } else {
        println!("Ctrl-D to exit.\n");
        run_plain(&mut env);
    }
}

fn dispatch(line: &str, env: &mut Environment) {
    match eval(line, env) {
        Ok(result) => println!("{result}"),
        Err(e) => eprintln!("error: {e}"),
    }
}

/// History-enabled loop. `rustyline` handles raw-mode editing, Up/Down
/// recall, and falls back to plain reads when stdin is not a TTY.
fn run_with_history(cfg: &Config, env: &mut Environment) {
    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("line editor unavailable ({e}); falling back to plain input");
            return run_plain(env);
        }
    };
    let _ = rl.load_history(&cfg.history_file);

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "quit" || trimmed == "exit" {
                    break;
                }
                let _ = rl.add_history_entry(trimmed);
                dispatch(trimmed, env);
            }
            Err(ReadlineError::Interrupted) => continue, // Ctrl-C
            Err(ReadlineError::Eof) => break,            // Ctrl-D
            Err(e) => {
                eprintln!("input error: {e}");
                break;
            }
        }
    }

    if let Err(e) = rl.save_history(&cfg.history_file) {
        eprintln!("warning: could not save history: {e}");
    }
}

/// Plain line reader used when `history = false` (no raw-mode editing).
fn run_plain(env: &mut Environment) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        let _ = stdout.flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                break; // EOF
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("input error: {e}");
                break;
            }
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" || line == "exit" {
            break;
        }
        dispatch(line, env);
    }
}
