# Vynglish Commands Reference

Generated from pattern table (v0.1.0).

## Core Commands

### and

Logical AND of two conditions.

**Usage:** `<condition> and <condition>`

**Example:**
```
if x > 5 and x < 10
```

**Error:** Usage: <condition> and <condition>

### ask

Read a line of input from the user and store it in a variable.

**Usage:** `ask "<question>" and remember it as <name>`

**Example:**
```
ask "What is your name?" and remember it as name
```

**Error:** Usage: ask "<question>" and remember it as <name>

### ask the system for

Ask the machine to do something, such as reading the clock.

**Usage:** `ask the system for "<ability>"`

**Example:**
```
ask the system for "current time"
```

**Error:** Usage: ask the system for "<ability>"

### ask the system for remember

Ask the machine to do something and keep the answer under a name.

**Usage:** `ask the system for "<ability>" and remember it as <name>`

**Example:**
```
ask the system for "current date" and remember it as today
```

**Error:** Usage: ask the system for "<ability>" and remember it as <name>

### ask the system for with

Ask the machine to do something and give it the arguments it needs.

**Usage:** `ask the system for "<ability>" with <args...>`

**Example:**
```
ask the system for "text length" with "hello"
```

**Error:** Usage: ask the system for "<ability>" with <args...>

### ask the system for with remember

Ask the machine to do something and keep the answer under a name.

**Usage:** `ask the system for "<ability>" with <arg> and remember it as <name>`

**Example:**
```
ask the system for "text length" with "hello" and remember it as n
```

**Error:** Usage: ask the system for "<ability>" with <arg> and remember it as <name>

### divided by

Divide a by b. Division by zero gives undefined.

**Usage:** `<a> divided by <b>`

**Example:**
```
set result to 10 divided by 2
```

**Error:** Usage: <number> divided by <number>

### end

End a block (if, repeat, teach).

**Usage:** `end`

**Example:**
```
if x > 5
    print "big"
end
```

**Error:** Usage: end (to close a block)

### execute reverse

Run the program from bottom to top. Only works in script files.

**Usage:** `execute reverse`

**Example:**
```
print "first"
print "second"
execute reverse
```

**Error:** Usage: execute reverse (script files only)

### if

Start a conditional block. End with 'end'. Use 'otherwise' for else branch.

**Usage:** `if <condition>
    ...
otherwise
    ...
end`

**Example:**
```
if x is greater than 5
    print "big"
otherwise
    print "small"
end
```

**Error:** Usage: if <condition> ... otherwise ... end

### is at least

Compare if a is greater than or equal to b.

**Usage:** `<a> is at least <b>`

**Example:**
```
if x is at least 5
```

**Error:** Usage: <value> is at least <value>

### is at most

Compare if a is less than or equal to b.

**Usage:** `<a> is at most <b>`

**Example:**
```
if x is at most 10
```

**Error:** Usage: <value> is at most <value>

### is equal to

Compare if a equals b.

**Usage:** `<a> is equal to <b>`

**Example:**
```
if name is equal to "Alice"
```

**Error:** Usage: <value> is equal to <value>

### is greater than

Compare if a is greater than b.

**Usage:** `<a> is greater than <b>`

**Example:**
```
if x is greater than 5
```

**Error:** Usage: <value> is greater than <value>

### is less than

Compare if a is less than b.

**Usage:** `<a> is less than <b>`

**Example:**
```
if x is less than 10
```

**Error:** Usage: <value> is less than <value>

### is not equal to

Compare if a does not equal b.

**Usage:** `<a> is not equal to <b>`

**Example:**
```
if x is not equal to 0
```

**Error:** Usage: <value> is not equal to <value>

### minus

Subtract b from a.

**Usage:** `<a> minus <b>`

**Example:**
```
set result to 10 minus 3
```

**Error:** Usage: <number> minus <number>

### not

Logical NOT of a condition.

**Usage:** `not <condition>`

**Example:**
```
if not done
```

**Error:** Usage: not <condition>

### or

Logical OR of two conditions.

**Usage:** `<condition> or <condition>`

**Example:**
```
if x < 0 or x > 100
```

**Error:** Usage: <condition> or <condition>

### otherwise

Else branch for an if statement.

**Usage:** `otherwise`

**Example:**
```
if x is greater than 5
    print "big"
otherwise
    print "small"
end
```

**Error:** Usage: otherwise (inside an if block)

### plus

Add two numbers.

**Usage:** `<a> plus <b>`

**Example:**
```
set result to 2 plus 3
```

**Error:** Usage: <number> plus <number>

### print

Show text, a value, or the result of a command on the screen. Quoted text is shown exactly as written; a bare word shows its variable when it has one.

**Usage:** `print <text>`

**Example:**
```
print "hello world"
print name
print 23 divided by 6
```

**Error:** Usage: print <text>

### repeat_for_each

Repeat a block for each item in a list.

**Usage:** `repeat for each <item> in <list>
    ...
end`

**Example:**
```
repeat for each name in names
    print name
end
```

**Error:** Usage: repeat for each <name> in <list> ... end

### repeat_times

Repeat a block a fixed number of times.

**Usage:** `repeat <n> times
    ...
end`

**Example:**
```
repeat 3 times
    print "hello"
end
```

**Error:** Usage: repeat <number> times ... end

### repeat_while

Repeat a block while a condition is true.

**Usage:** `repeat while <condition>
    ...
end`

**Example:**
```
repeat while x is less than 10
    set x to x plus 1
end
```

**Error:** Usage: repeat while <condition> ... end

### set

Create or change a variable.

**Usage:** `set <name> to <value>`

**Example:**
```
set x to 42
set name to "Alice"
```

**Error:** Usage: set <name> to <value>

### stop

Leave the current loop immediately.

**Usage:** `stop`

**Example:**
```
repeat 10 times
    if x is equal to 5
        stop
    end
end
```

**Error:** Usage: stop (inside a repeat block)

### teach

Define a new custom command. End with 'end'. Use <angle brackets> for placeholders.

**Usage:** `teach "<pattern>" [:]
    ...
end`

**Example:**
```
teach "greet <name>"
    print "Hello, <name>"
end
```

**Error:** Usage: teach "<pattern>" ... end

### times

Multiply two numbers.

**Usage:** `<a> times <b>`

**Example:**
```
set result to 6 times 7
```

**Error:** Usage: <number> times <number>

### use

Load a module. The module's commands become available after this line.

**Usage:** `use <module>`

**Example:**
```
use math
use mytools
```

**Error:** Usage: use <module>

### wait

Pause execution for n seconds.

**Usage:** `wait <n> seconds`

**Example:**
```
wait 2 seconds
```

**Error:** Usage: wait <number> seconds

## Session Commands

These words belong to the prompt, not to a program.

### copyright

Show who owns Vynglish and under what licence.

**Usage:** `copyright`

**Example:**
```
copyright
```

**Error:** Usage: copyright

### credits

Show who made Vynglish and what it is built from.

**Usage:** `credits`

**Example:**
```
credits
```

**Error:** Usage: credits

### exit

Leave Vynglish. Ctrl-D does the same, and so does Ctrl-Z then Enter on Windows.

**Usage:** `exit`

**Example:**
```
exit
```

**Error:** Usage: exit

### help

Open help. Type the name of a command to read about it. Leave with q, quit or exit.

**Usage:** `help
help <name>`

**Example:**
```
help
help print
```

**Error:** Usage: help [name]

### history

Show the lines you have typed this session, numbered. 'history <n>' shows the last n.

**Usage:** `history
history <n>`

**Example:**
```
history
history 5
```

**Error:** Usage: history [count]

### license

Show the MIT licence Vynglish is released under.

**Usage:** `license`

**Example:**
```
license
```

**Error:** Usage: license

### paste

Gather lines until 'end paste', then run them as one program, like a script file.

**Usage:** `paste
    ...
end paste`

**Example:**
```
paste
print "hello"
end paste
```

**Error:** Usage: paste ... end paste

### quit

Leave Vynglish. The same as exit.

**Usage:** `quit`

**Example:**
```
quit
```

**Error:** Usage: quit

## Module: math

### absolute value of

Absolute value of n.

**Usage:** `absolute value of <n>`

**Example:**
```
set result to absolute value of -5
```

**Error:** Usage: absolute value of <number>

### biggest of

The larger of two numbers.

**Usage:** `biggest of <a> and <b>`

**Example:**
```
set result to biggest of 3 and 7
```

**Error:** Usage: biggest of <number> and <number>

### factorial of

Compute the factorial of n.

**Usage:** `factorial of <n>`

**Example:**
```
set result to factorial of 5
```

**Error:** Usage: factorial of <number>

### factors of

Find all factors of n.

**Usage:** `factors of <n>`

**Example:**
```
set result to factors of 12
```

**Error:** Usage: factors of <number>

### log base

Logarithm of n with base b.

**Usage:** `log base <b> of <n>`

**Example:**
```
set result to log base 2 of 8
```

**Error:** Usage: log base <number> of <number>

### log of

Natural logarithm of n. Base e.

**Usage:** `log of <n>`

**Example:**
```
set result to log of 10
```

**Error:** Usage: log of <number>

### percent of

Compute p percent of n.

**Usage:** `<p> percent of <n>`

**Example:**
```
set result to 20 percent of 150
```

**Error:** Usage: <number> percent of <number>

### round

Round n to the nearest integer.

**Usage:** `round <n>`

**Example:**
```
set result to round 3.7
```

**Error:** Usage: round <number>

### smallest of

The smaller of two numbers.

**Usage:** `smallest of <a> and <b>`

**Example:**
```
set result to smallest of 3 and 7
```

**Error:** Usage: smallest of <number> and <number>

### square root of

Compute the square root of n. Perfect squares are exact; others are approximate.

**Usage:** `square root of <n>`

**Example:**
```
set result to square root of 16
set approx to square root of 2
```

**Error:** Usage: square root of <number>

### to the power of

Raise a to the power of b.

**Usage:** `<a> to the power of <b>`

**Example:**
```
set result to 2 to the power of 10
```

**Error:** Usage: <number> to the power of <number>

## Module: text

### contains

Check if text contains x.

**Usage:** `<text> contains <x>`

**Example:**
```
if "hello world" contains "world"
```

**Error:** Usage: <text> contains <text>

### join

Join two texts together.

**Usage:** `join <a> and <b>`

**Example:**
```
set result to join "hello" and "world"
```

**Error:** Usage: join <text> and <text>

### length of

Get the length of a text.

**Usage:** `length of <text>`

**Example:**
```
set n to length of "hello"
```

**Error:** Usage: length of <text>

### lowercase

Convert text to lowercase.

**Usage:** `lowercase <text>`

**Example:**
```
set result to lowercase "HELLO"
```

**Error:** Usage: lowercase <text>

### replace

Replace all occurrences of x with y in text.

**Usage:** `replace <x> with <y> in <text>`

**Example:**
```
set result to replace "world" with "Vynglish" in "hello world"
```

**Error:** Usage: replace <text> with <text> in <text>

### split

Split text by delimiter x into a list.

**Usage:** `split <text> by <x>`

**Example:**
```
set words to split "a,b,c" by ","
```

**Error:** Usage: split <text> by <text>

### uppercase

Convert text to uppercase.

**Usage:** `uppercase <text>`

**Example:**
```
set result to uppercase "hello"
```

**Error:** Usage: uppercase <text>

## Module: list

### add

Add an item to the end of a list.

**Usage:** `add <item> to <list>`

**Example:**
```
add 4 to nums
```

**Error:** Usage: add <item> to <list>

### first of

Get the first item of a list (1-indexed).

**Usage:** `first of <list>`

**Example:**
```
set x to first of nums
```

**Error:** Usage: first of <list>

### item of

Get the item at the given 1-based index.

**Usage:** `item <index> of <list>`

**Example:**
```
set x to item 2 of nums
```

**Error:** Usage: item <number> of <list>

### last of

Get the last item of a list.

**Usage:** `last of <list>`

**Example:**
```
set x to last of nums
```

**Error:** Usage: last of <list>

### length of list

Get the number of items in a list.

**Usage:** `length of <list>`

**Example:**
```
set n to length of nums
```

**Error:** Usage: length of <list>

### make a list called

Create an empty list.

**Usage:** `make a list called <name>`

**Example:**
```
make a list called items
```

**Error:** Usage: make a list called <name>

### make a list called with

Create a list with initial items (comma-separated, use 'and' before last).

**Usage:** `make a list called <name> with <items..>`

**Example:**
```
make a list called nums with 1, 2 and 3
```

**Error:** Usage: make a list called <name> with <items..>

### remove

Remove the first occurrence of an item from a list.

**Usage:** `remove <item> from <list>`

**Example:**
```
remove 2 from nums
```

**Error:** Usage: remove <item> from <list>

### reverse

Reverse a list in place.

**Usage:** `reverse <list>`

**Example:**
```
reverse nums
```

**Error:** Usage: reverse <list>

### sort

Sort a list in place.

**Usage:** `sort <list>`

**Example:**
```
sort nums
```

**Error:** Usage: sort <list>

## Module: random

### pick random from

Pick a random element from a list.

**Usage:** `pick random from <list>`

**Example:**
```
set x to pick random from items
```

**Error:** Usage: pick random from <list>

### random number between

Generate a random integer between a and b (inclusive).

**Usage:** `random number between <a> and <b>`

**Example:**
```
set x to random number between 1 and 10
```

**Error:** Usage: random number between <number> and <number>

## Module: time

### current date

Get the current date as a string (YYYY-MM-DD).

**Usage:** `current date`

**Example:**
```
print current date
```

**Error:** Usage: current date

### current time

Get the current time as a string (HH:MM:SS).

**Usage:** `current time`

**Example:**
```
print current time
```

**Error:** Usage: current time

### time since

Get seconds elapsed since a start time (from clock ticks).

**Usage:** `time since <start>`

**Example:**
```
set start to current time
wait 2 seconds
print time since start
```

**Error:** Usage: time since <number>

