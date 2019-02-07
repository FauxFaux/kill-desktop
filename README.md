## kill-desktop

[![Build status](https://api.travis-ci.org/FauxFaux/kill-desktop.png)](https://travis-ci.org/FauxFaux/kill-desktop)
[![](https://img.shields.io/crates/v/kill-desktop.svg)](https://crates.io/crates/kill-desktop)

`kill-desktop` helps you exit your graphical applications without losing
 data. It can ask applications to exit, tell them to exit, and exit them.

### Config

There must be a config file. The app prefers xdg directories, then `.`.

Start it with no config, and it will make and use one:
```
I couldn't find a config file at any of these locations:
 * "/home/faux/.config/kill-desktop/config.toml"
 * "/home/faux/.kill-desktop.toml"
 * "kill-desktop.toml"

So.. I'm going to make you one. I hope you like it:
 * "/home/faux/.config/kill-desktop/config.toml"
Done!
```


### Demo

`kill-desktop` presents the status of the current system,
  and offers you some options:
```
$ kill-desktop
📫 gedit - Untitled Document 1 - gedit
📫 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📫 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - kill-desktop [~/code/kill-desktop] - .../src/main.rs [kill-desktop] - IntelliJ
```

Here, we can see a load of applications running. This view is updated
 in real-time.

You are asked for an action. Let's ask everyone to shut down nicely:

```
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit? a
Asking everyone to quit.
```

The apps start shutting down:

```
📫 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - kill-desktop [~/code/kill-desktop] - .../src/main.rs [kill-desktop] - IntelliJ
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit?
```

`gedit` has gone, `slack` and `Spotify` are thinking about it.

```
📭 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - kill-desktop [~/code/kill-desktop] - .../src/main.rs [kill-desktop] - IntelliJ
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit?
```

`google-chrome` has woken up and is thinking about it.

```
📭 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - IntelliJ IDEA
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit?
```

They're going down!

```
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - IntelliJ IDEA
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit?
```

```
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📭 sun-awt-X11-XFramePeer - IntelliJ IDEA
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit?
```

... so slowly ...

```
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit?
```

```
📭 slack - Slack
```

Aha, just slack is left, and it's closed its window. It hasn't exited,
though. That would be what we want. Let's *tell* it to quit.

```
Apply to all: [a]sk to exit/[a]lt+f4, [t]ell to exit/[t]erm, [k]ill, or [q]uit? t
Telling everyone to quit.
```

```
No applications found, exiting.
```

At last, it went away, and we can confidently reboot.
