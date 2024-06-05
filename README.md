# Unix Shell in Rust

This is a basic shell written in Rust. It provides a command-line interface to interact with an UNIX operating system and execute commands.

## Features:

- Command execution: Execute system commands with arguments.
- Built-in commands:
    - cd: Change the current working directory.
    - pwd: Print the current working directory.
    - exit: Exit the shell.
    - history: Show the command history.
- Pipelines: Chain multiple commands together using pipes (|). The output of one command is piped as the input to the next.
- I/O redirection:
    - \>: Redirect standard output (stdout) to a file.
    - <: Redirect standard input (stdin) from a file.
    - 2>: Redirect standard error (stderr) to a file.
- Background execution: Run a command in the background by appending an ampersand (&) to the end of the command line.
- Command history: Keeps track of previously entered commands and allows navigation through them using the up and down arrow keys.
- Command completion: Suggests possible completions for commands and filenames.