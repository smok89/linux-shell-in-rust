use std::borrow::Cow::{self, Borrowed, Owned};
use std::{boxed, env};
use std::process::*;
use std::path::Path;
use nix::sys::signal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::fs::File;
use rustyline::completion::FilenameCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::HistoryHinter;
use rustyline::validate::MatchingBracketValidator;
use rustyline::{CompletionType, Config, EditMode, Editor, Hinter, Helper, Validator, Completer};
use rustyline::config::ColorMode;


#[macro_use]
extern crate lazy_static;

// Atomic flag to track the interrupt signal
lazy_static! {
    static ref INTERRUPTED: AtomicBool = AtomicBool::new(false);
}

// Handler function for SIGINT
extern "C" fn handle_sigint(_sig: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

#[derive(Helper, Completer, Hinter, Validator)]
struct MyHelper {
    #[rustyline(Completer)]
    completer: FilenameCompleter,
    highlighter: MatchingBracketHighlighter,
    #[rustyline(Validator)]
    validator: MatchingBracketValidator,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
    colored_prompt: String,
}

impl Highlighter for MyHelper {}


fn main() {
    // Set the SIGINT handler using nix's sigaction
    unsafe {
        signal::sigaction(
            signal::SIGINT, &signal::SigAction::new(
                signal::SigHandler::Handler(handle_sigint),
                signal::SaFlags::empty(),
                signal::SigSet::empty(),
            ),
        )
        .expect("Error setting SIGINT handler");
    }

    // History implementation with rustyline
    let config = Config::builder()
        .history_ignore_dups(false).unwrap()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .color_mode(ColorMode::Forced).build();

    let h = MyHelper {
        completer: FilenameCompleter::new(),
        highlighter: MatchingBracketHighlighter::new(),
        hinter: HistoryHinter::new(),
        colored_prompt: "".to_owned(),
        validator: MatchingBracketValidator::new(),
    };

    let mut rl = Editor::with_config(config).unwrap(); // new editor

    rl.set_helper(Some(h));

    let home = env::var("HOME").unwrap(); // get home path
    if rl.load_history(&format!("{}/.rust_shell_history", home)).is_err() {
        println!("No previous history.");
        File::create(format!("{}/.rust_shell_history", home)).expect("Couldn't create history file");
    }

    // loop to get and execute commands
    loop { 
        // Check if the interrupt flag is set and handle it
        if INTERRUPTED.load(Ordering::SeqCst) {
            println!("\nReceived Ctrl+C. Use 'exit' command to quit the shell.");
            INTERRUPTED.store(false, Ordering::SeqCst); // Reset the flag
            continue; // Get back to the start of the loop
        }

        /* This was the first implementation, but as I found out about rustyline, I used it instead.
        rustyline can handle Ctrl+C, moving the cursor left and right keys, and search the histor with up and down.
        At first, I wanted to implement those features from scratch, but time is lacking :')

        // we want the prompt to be displayed again after executing a command
        let current_dir = env::current_dir().unwrap(); // get current working directory
        print!("{} > ", current_dir.to_str().unwrap()); // display the prompt with cwd
        let _ = stdout().flush(); // displays everything currently in the buffer : displays the prompt immediately

        let mut input = String::new();
        match stdin().read_line(&mut input) { // we reed the stdin and store the result in input
            Ok(n) => {
                if n == 1 { continue; } // nothing was provided (only reading \n)
            },
            Err(e) => {
                eprintln!("{}",e);
                continue; // we go back to the beggining of the loop to ask for a valid input
            }
        }
        */

        // Implementation with rustyline

        let input;
        let current_dir = env::current_dir().unwrap(); // get current working directory
        let readline = rl.readline(format!("{} > ", current_dir.to_str().unwrap()).as_str());
        match readline {
            Ok(line) => {
                if line.len() == 0 {continue} // avoid crashing if we press Enter with no input
                let _ = rl.add_history_entry(line.as_str());
                input = line;
                if rl.save_history(&format!("{}/.rust_shell_history", home)).is_err() {
                    println!("Could not save history.");
                };
            },
            Err(ReadlineError::Interrupted) => {
                println!("Received Ctrl+C. Use 'exit' command to quit the shell.");
                continue
            },
            Err(ReadlineError::Eof) => {
                println!("Received Ctrl+D.");
                continue
            },
            Err(err) => {
                println!("Error: {:?}", err);
                continue
            }
        }

        let mut commands = input.trim() // removes trailing whitespaces
                                                            .split(" | ") // handling pipes
                                                            .peekable(); // create iterator that does not consumes elements 
        let mut previous_command = None; // used for pipes

        while let Some(command) = commands.next() {

            let mut parts = command.trim().split_whitespace(); // get an iterator with command name and arguments
            let command = parts.next().unwrap(); // get the command name ; unwrap because it is a Result
            let mut args = parts; // get the rest = the arguments

            match command {
                // first we need to check if the command name is a shell builtin
                "cd" => {
                    let new_dir = args.next().map_or("/", |x| x); // if no arg was given, use "/" as default destination
                    if !args.next().is_none() { // not expecting more than 1 argument
                        eprintln!("Too much arguments provided for cd.");
                        break; // quit the while loop and get back to the main loop
                    }
                    let root = Path::new(new_dir); // the next function requires a &Path as arg
                    if let Err(e) = env::set_current_dir(&root) {
                        eprintln!("{}",e);
                    }

                    previous_command = None;
                },
                "pwd" => {
                    if !args.next().is_none() { // not expecting any arg
                        eprintln!("No argument required for pwd.");
                        break; // quit the while loop and get back to the main loop
                    }
                    let working_dir = env::current_dir().unwrap();
                    println!("cwd : {}", working_dir.to_str().unwrap());

                    previous_command = None; // TODO : handle the piping to a following command
                },
                "exit" => return, // quit the program

                // if it's not a shell builtin:
                command => {
                    let stdin = previous_command.map_or(Stdio::inherit(), 
                        |output: Child| Stdio::from(output.stdout.unwrap())
                    );

                    let stdout = if commands.peek().is_some() {
                        Stdio::piped() // pipes the output to the following command
                    } else {
                        Stdio::inherit() // output is printed in the terminal
                    };

                    let output = Command::new(command)
                        .args(args)
                        .stdin(stdin)
                        .stdout(stdout)
                        .spawn(); // output is an handle to the child process created

                    match output {
                        Ok(output) => { previous_command = Some(output); },
                        Err(e) => {
                            previous_command = None;
                            eprintln!("{}",e);
                        },
                    }
                }
            }
        }

        if let Some(mut final_command) = previous_command {
            let _ = final_command.wait(); // wait for all processes to finish before printing the prompt again
        }
    }
}
