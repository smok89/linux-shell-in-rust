use std::env;
use std::process::*;
use std::path::Path;
use nix::sys::signal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::fs::File;
use rustyline::completion::FilenameCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::HistoryHinter;
use rustyline::validate::MatchingBracketValidator;
use rustyline::{CompletionType, Config, EditMode, Editor, Hinter, Helper, Validator, Completer};
use rustyline::config::ColorMode;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};


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
    #[rustyline(Validator)]
    validator: MatchingBracketValidator,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
}

impl Highlighter for MyHelper {}

fn print_history(rl: &mut Editor<MyHelper, rustyline::history::FileHistory>) {
    let history = rl.history();
    for (index, entry) in history.iter().enumerate() {
        println!("{}: {}", index + 1, entry);
    }
}

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
        .completion_type(CompletionType::Circular)
        .edit_mode(EditMode::Emacs)
        .color_mode(ColorMode::Forced).build();

    let h = MyHelper {
        completer: FilenameCompleter::new(),
        hinter: HistoryHinter::new(),
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
                input = line;
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

        let trimmed = input.trim(); // removes trailing whitespaces

        // save the command in the history ; history and pwd are not saved
        if trimmed != "history" && trimmed != "pwd" { 
            let _ = rl.add_history_entry(trimmed);
                    
            if rl.save_history(&format!("{}/.rust_shell_history", home)).is_err() {
                println!("Could not save history.");
            };
        } 

        // check if the command runs in the background ('&' symbol at the end)
        let runs_background = trimmed.ends_with('&');
        let commands_without_ampersand = if runs_background {
            &trimmed[..trimmed.len()-1] // remove '&'
        } else { 
            trimmed
        };        

        let mut commands = commands_without_ampersand.split(" | ") // handling pipes
                                                            .peekable(); // create iterator that does not consumes elements 
        let mut previous_command = None; // used for pipes

        while let Some(command) = commands.next() { // iterate over the commands separated by pipes

            let mut parts = command.trim().split_whitespace(); // get an iterator with command name and arguments
            let command = parts.next().unwrap(); // get the command name = first element ; unwrap because it is a Result
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

                    previous_command = None; 
                },
                "exit" => {
                    if !args.next().is_none() { // not expecting any arg
                        eprintln!("No argument required for pwd.");
                        break; // quit the while loop and get back to the main loop
                    };
                    return; // quit the program
                }, 
                
                "history" => {
                    if !args.next().is_none() { // not expecting any arg
                        eprintln!("No argument required for pwd.");
                        break; // quit the while loop and get back to the main loop
                    };
                    previous_command = None; 
                    print_history(&mut rl);
                },


                // if it's not a shell builtin:
                command => {

                    // default values for stdin, stdout and stderr (considering pipes)
                    let mut stdin = previous_command.map_or(Stdio::inherit(), 
                    |output: Child| Stdio::from(output.stdout.unwrap())
                    );

                    let mut stdout = if commands.peek().is_some() {
                        Stdio::piped() // pipes the output to the following command
                    } else {
                        if !runs_background 
                            {Stdio::inherit()} // output is printed in the terminal
                        else 
                            {Stdio::null()} // output is sent to /dev/null for background processes
                    };

                    let mut stderr = Stdio::inherit();

                    // check for redirection (>, <, 2>)
                    let mut clone_args = args.clone().peekable();

                    loop {
                        match clone_args.peek() {
                            Some(&arg) => {
                                if arg == ">" {
                                    // handle standard output redirection (>)
                                    clone_args.next(); // skip ">"
                                    let filename = Path::new(clone_args.next().unwrap().trim());
                                    stdout = Stdio::from(File::create(filename).unwrap());
                                    break;
                                } else if arg == "<" {
                                    // handle standard input redirection (<)
                                    clone_args.next(); // skip "<"
                                    let filename = Path::new(clone_args.next().unwrap().trim());
                                    stdin = Stdio::from(File::open(filename).unwrap());
                                    break;
                                } else if arg == "2>" {
                                    // handle standard error redirection (2>)
                                    clone_args.next(); // skip "2>"
                                    let filename = Path::new(clone_args.next().unwrap().trim());
                                    stderr = Stdio::from(File::create(filename).unwrap());
                                    break;
                                } else {
                                    if clone_args.next().is_none() {
                                        break;
                                    }
                                }
                            }
                            None => {break;},
                        }
                        
                    }

                    let mut args = args.peekable();

                    let mut truncated_args: Vec<&str> = Vec::new();

                    loop {
                        match args.peek() {
                            Some(&arg) => {
                                if arg == ">" || arg == "<" || arg == "2>" {
                                    // Truncate arguments before redirection symbol
                                    break;
                                } else {
                                    truncated_args.push(args.next().unwrap());
                                }
                            }
                            None => {
                                break;
                            }
                        }
                    }

                    let output = Command::new(command)
                        .args(truncated_args)
                        .stdin(stdin)
                        .stdout(stdout)
                        .stderr(stderr)
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
            if !runs_background {
            let _ = final_command.wait(); // wait for all processes to finish before printing the prompt again
            }
            // if running in background, don't wait
        }

        // we need to check for any child process in background ; ressources are freed only after a wait
        // if we don't wait for a child process, its pid is never freed
        
        loop {
            let wait_result = waitpid(None, Some(WaitPidFlag::WNOHANG));
            match wait_result {
                Ok(WaitStatus::StillAlive) => {
                    // No child process has exited yet, go back to the beginning of the main loop
                    break;
                }
                Ok(_wait_status) => { 
                    // A child has exited, check if there is another
                    continue;
                    }  
                Err(_) => { // e.g. no child process
                    break;
                }     
            }
        }
        

    }
}
