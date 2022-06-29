# transaction parser project

A simple example project which parses a csv to enact transactions on client data and produce a description of the client account.

Output is generated to stdout; logging is performed to stderr

# Notes:

Docs have been written; they can be generated with `cargo doc`

Several errors are expected on stderr when running with the test data, transaction_data.csv, file in the repo.

There are other tests which can be run with `cargo test`

# Lingering Questions:

Precision output is limited to 4 digits after the decimal.  In case extra precision is input, or if future operations were added which necessitate more data to accurately track monetary ammounts, 'Banker's Rounding' is applied when data is output.

I wasn't sure rather disputes could be made against both deposits and withdrawals.  After pondering it, I decided based on descriptions available that disputes were made not by clients with wealth in the system, but rather by their services.  Therefore, only disputes against deposits are executed.  Trying to dispute a withdrawal will prompt a warning and be ignored.

# Where to improve

TODO
- panic vs ret exit code?  I have one such...  exit code enum?
- write_csv doesn't work if the file given to it doesn't ALREADY exist in the file system

Nice to Have
- modify warnings and errors to specify where they originate
- command_handler
-   move closures to functions?
-   use constants for duplicated static strings
- input validation on program arguments.  Make sure it is a valid file path in the current OS.  Maybe change the type being returned and sent via the parse_csv function

Extra
- readme.md
- Metrics
- Features to control logging
 - review panic conditions and choice of warning vs error
- only add to queue when it is lower than a cli value
