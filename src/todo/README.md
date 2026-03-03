# Todo Module

Local task management with file-based storage.

## Concepts

- **TodoFile**: A `.todo` file containing all tasks (e.g., `project.todo`, `SECURITY_AUDIT.todo`)
- **List**: A grouping within a file (e.g., "inbox", "bugs", "features")
- **Task**: An individual todo item with status, tags, assignee, etc.

## CLI

```bash
# Show all tasks grouped by list
zero todo

# View specific list
zero todo bugs

# Open a different todo file
zero todo open SECURITY.todo
zero todo close                    # Back to project.todo

# Add tasks
zero todo add "Fix crash"                    # → inbox (default)
zero todo add "Fix crash" bugs               # → bugs list
zero todo add "Add dark mode" features       # → features list

# Complete/manage
zero todo toggle 1 2 3               # Toggle status (open ↔ done)
zero todo remove 1
zero todo show 1

# Move/reorder tasks
zero todo move 3 1                   # Move task #3 after #1
zero todo move 3 top                 # Move task #3 to top of list
zero todo move 3 last                # Move task #3 to bottom
zero todo move 3 top --list inbox    # Move to top of inbox

# Update (edit multiple fields)
zero todo update 1 --text "New text" --assigned bob --due tomorrow
zero todo update 1 --add-tag urgent --list bugs

# Search across all .todo files (via FFI/GUI)
# Uses TypeIndex bitmap to find .todo files instantly, then searches tasks
```

## Suggested Lists

| List | Use for |
|------|---------|
| `inbox` | Unsorted tasks (default) |
| `bugs` | Defects, issues to fix |
| `features` | New functionality |
| `refactor` | Code improvements |
| `security` | Security concerns |
| `docs` | Documentation tasks |

## Rust API

```rust
use zero::todo::{TodoManager, Task};

// Open current context (or project.todo)
let mut manager = TodoManager::open_current()?;

// Or open specific file
let mut manager = TodoManager::open_file("SECURITY.todo")?;

// Add tasks
manager.add("Fix bug", Some("bugs"))?;
manager.add("New feature", Some("features"))?;

// Complete
manager.complete(1)?;
manager.complete_many(&[2, 3])?;

// Query
let bugs = manager.tasks_in_list("bugs");
let open = manager.open_tasks();
```

## Storage

- Files stored in working directory (e.g., `./project.todo`)
- Format: postcard binary, atomic writes
- Context (current file) stored in `~/.config/zero/todo-context`

## Search API

Search across all `.todo` files in indexed directories:

```rust
use zero::todo::{TodoSearchOptions, search_todos, find_todo_files};

// Find all .todo files in directories
let todo_files = find_todo_files(&[PathBuf::from("~/projects")]);

// Search with options
let options = TodoSearchOptions::with_query("fix crash")
    .with_tag("urgent")
    .with_status("open")
    .with_limit(50);

let results = search_todos(&todo_files, &options);

for result in results {
    println!("{}: {} ({})", result.file_name, result.task.text, result.task.list);
}
```

### FFI

```c
// Preferred: Search all indexed .todo files (uses TypeIndex bitmap - instant)
char* results = zero_todo_search_indexed(
    index,  // from zero_index_load()
    "{\"query\": \"fix\", \"tag\": \"urgent\", \"status\": \"open\"}"  // options JSON
);
// Returns JSON array of TaskSearchResult

// Alternative: Search specific .todo files (if you have explicit paths)
char* results = zero_todo_search(
    "[\"~/projects/app/project.todo\", \"~/work/api.todo\"]",  // paths JSON
    "{\"query\": \"fix\", \"tag\": \"urgent\", \"status\": \"open\"}"  // options JSON
);
```

## Tests

```bash
cargo test todo::
```
