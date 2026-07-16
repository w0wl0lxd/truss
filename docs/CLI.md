# CLI Reference

> Generated from `truss --help`.

# `truss`

```text
Rust project scaffolder with template sync and local registries

Usage: truss <COMMAND>

Commands:
  new        Create a new project from a template
  sync       Sync a project to a template
  check      Check for drift against a template
  update     Apply upstream template changes with a 3-way merge
  extract    Reverse-scaffold an existing project into a reusable pack
  define     List variables expected by a template pack
  templates  List embedded and registry templates
  types      List and inspect project-type presets
  registry   Manage the local template registry
  member     Manage workspace members
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## `truss new`

```text
Create a new project from a template

Usage: truss new [OPTIONS] [NAME]

Arguments:
  [NAME]  

Options:
  -t, --template <TEMPLATE>  
  -p, --path <PATH>          
      --author <AUTHOR>      
      --license <LICENSE>    
      --edition <EDITION>    
      --type <TYPE>          Project-type preset to use
      --define <KEY=VALUE>   Provide a prompt answer as KEY=VALUE (repeatable)
      --dry-run              Preview planned writes without modifying the project
  -h, --help                 Print help
```

## `truss sync`

```text
Sync a project to a template

Usage: truss sync [OPTIONS]

Options:
  -p, --path <PATH>          
  -t, --template <TEMPLATE>  
      --author <AUTHOR>      
      --license <LICENSE>    
      --edition <EDITION>    
      --type <TYPE>          Project-type preset to use
      --define <KEY=VALUE>   Provide a prompt answer as KEY=VALUE (repeatable)
      --dry-run              Preview planned writes without modifying the project
      --protect <PROTECT>    Relative paths that must not be overwritten (repeatable)
  -h, --help                 Print help
```

## `truss check`

```text
Check for drift against a template

Usage: truss check [OPTIONS]

Options:
  -p, --path <PATH>          
  -t, --template <TEMPLATE>  
      --author <AUTHOR>      
      --license <LICENSE>    
      --edition <EDITION>    
      --type <TYPE>          Project-type preset to use
      --define <KEY=VALUE>   Provide a prompt answer as KEY=VALUE (repeatable)
  -h, --help                 Print help
```

## `truss update`

```text
Apply upstream template changes with a 3-way merge

Usage: truss update [OPTIONS]

Options:
  -p, --path <PATH>           
  -t, --template <TEMPLATE>   
      --author <AUTHOR>       
      --license <LICENSE>     
      --edition <EDITION>     
      --type <TYPE>           Project-type preset to use
      --define <KEY=VALUE>    Provide a prompt answer as KEY=VALUE (repeatable)
      --dry-run               Preview planned writes without modifying the project
      --write-conflicts       Write conflict markers instead of failing on conflicts
      --base <DIR>            Use a local directory as the base snapshot
      --base-template <NAME>  Use a template as the base snapshot
      --protect <PROTECT>     Relative paths that must not be overwritten (repeatable)
  -h, --help                  Print help
```

## `truss extract`

```text
Reverse-scaffold an existing project into a reusable pack

Usage: truss extract [OPTIONS] --source <SOURCE> --pack <PACK>

Options:
  -s, --source <SOURCE>    Source project directory to extract from
  -p, --pack <PACK>        Destination directory for the generated pack
      --force              Overwrite the destination directory if it already exists
      --skip-prompts       Do not generate a prompt manifest stub in the pack
      --value <KEY=VALUE>  Provide a custom replacement as KEY=VALUE (repeatable)
  -h, --help               Print help
```

## `truss define`

```text
List variables expected by a template pack

Usage: truss define [OPTIONS]

Options:
  -t, --template <TEMPLATE>  Template or registry entry to inspect
  -h, --help                 Print help
```

## `truss templates`

```text
List embedded and registry templates

Usage: truss templates

Options:
  -h, --help  Print help
```

## `truss types`

```text
List and inspect project-type presets

Usage: truss types [OPTIONS]

Options:
      --details <DETAILS>  Show details for a specific preset
  -h, --help               Print help
```

## `truss registry`

```text
Manage the local template registry

Usage: truss registry <COMMAND>

Commands:
  list    List registry + embedded templates
  add     Add a local template source
  remove  Remove a user registry entry
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `truss registry list`

```text
List registry + embedded templates

Usage: truss registry list

Options:
  -h, --help  Print help
```

### `truss registry add`

```text
Add a local template source

Usage: truss registry add [OPTIONS] --source <SOURCE> <NAME>

Arguments:
  <NAME>  

Options:
      --source <SOURCE>        
      --kind <KIND>            [default: dir] [possible values: dir, file, git, json]
      --force                  
      --target <TARGETS>       Relative destination paths (required for --kind file)
      --pointer <POINTER>      Git ref (branch, tag, or commit) to checkout for --kind git
      --subfolder <SUBFOLDER>  Subfolder inside the Git repository to use as the template root for --kind git
      --auth-env <AUTH_ENV>    Environment variable name containing an HTTPS token for --kind git
      --ssh-key <SSH_KEY>      Path to SSH private key for --kind git
  -h, --help                   Print help
```

### `truss registry remove`

```text
Remove a user registry entry

Usage: truss registry remove <NAME>

Arguments:
  <NAME>  

Options:
  -h, --help  Print help
```

## `truss member`

```text
Manage workspace members

Usage: truss member <COMMAND>

Commands:
  add     Add a crate to the workspace
  list    List workspace members
  remove  Remove a workspace member
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

### `truss member add`

```text
Add a crate to the workspace

Usage: truss member add [OPTIONS] --kind <KIND> <NAME>

Arguments:
  <NAME>  

Options:
      --kind <KIND>                [possible values: lib, bin]
      --member-path <MEMBER_PATH>  
  -p, --path <PATH>                Workspace root (defaults to current directory)
  -h, --help                       Print help
```

### `truss member list`

```text
List workspace members

Usage: truss member list [OPTIONS]

Options:
  -p, --path <PATH>  Workspace root (defaults to current directory)
  -h, --help         Print help
```

### `truss member remove`

```text
Remove a workspace member

Usage: truss member remove [OPTIONS] <NAME>

Arguments:
  <NAME>  

Options:
  -p, --path <PATH>  Workspace root (defaults to current directory)
      --delete       
  -h, --help         Print help
```

