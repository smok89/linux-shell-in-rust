extern crate libc;
#[macro_use]
extern crate lazy_static;

use std::env;
use std::io::*;
use std::process::*;
use std::path::Path;
use nix::sys::signal;
use std::sync::atomic::{AtomicBool, Ordering};

// Atomic flag to track the interrupt signal
lazy_static! {
    static ref INTERRUPTED: AtomicBool = AtomicBool::new(false);
}

    // Handler function for SIGINT

extern "C" fn handle_sigint(_sig: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
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

    // loop to get and execute commands
    loop { 
        // Check if the interrupt flag is set and handle it
        if INTERRUPTED.load(Ordering::SeqCst) {
            println!("\nReceived Ctrl+C. Use 'exit' command to quit the shell.");
            INTERRUPTED.store(false, Ordering::SeqCst); // Reset the flag
            continue; // Get back to the start of the loop
        }

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
