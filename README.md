# lx

```
╭┤/home/ce/Documents/GitHub/lx├───────────────────────────────────────────────────────────────────────────────────────────────╮
│src                  target               README.md            LICENSE              Cargo.lock           Cargo.toml          │
╰─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

it does that in your terminal basically

add this to your .bashrc

```
function lx(){
    /path/to/lx -u
    cd $(cat /tmp/lx$(tty))
}
```

or this to your config.fish

```
function lx
    /path/to/lx -u
    builtin cd (cat /tmp/lx(tty))
end
```
