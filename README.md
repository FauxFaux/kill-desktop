## kill-desktop

[![Build status](https://api.travis-ci.org/FauxFaux/kill-desktop.png)](https://travis-ci.org/FauxFaux/kill-desktop)
[![](https://img.shields.io/crates/v/kill-desktop.svg)](https://crates.io/crates/kill-desktop)

`kill-desktop` helps you exit your graphical applications without losing
 data. It can ask applications to exit, tell them to exit, and exit them.

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
Action?  [d]elete, [t]erm, [k]ill, [q]uit)? d
Asking everyone to quit.
```

The apps start shutting down:

```
📫 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - kill-desktop [~/code/kill-desktop] - .../src/main.rs [kill-desktop] - IntelliJ
Action?  [d]elete, [t]erm, [k]ill, [q]uit)?
```

`gedit` has gone, `slack` and `Spotify` are thinking about it.

```
📭 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - kill-desktop [~/code/kill-desktop] - .../src/main.rs [kill-desktop] - IntelliJ
Action?  [d]elete, [t]erm, [k]ill, [q]uit)?
```

`google-chrome` has woken up and is thinking about it.

```
📭 google-chrome - quad.pe - Google Chrome
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - IntelliJ IDEA
Action?  [d]elete, [t]erm, [k]ill, [q]uit)?
```

They're going down!

```
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📫 sun-awt-X11-XFramePeer - IntelliJ IDEA
Action?  [d]elete, [t]erm, [k]ill, [q]uit)?
```

```
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
📭 sun-awt-X11-XFramePeer - IntelliJ IDEA
Action?  [d]elete, [t]erm, [k]ill, [q]uit)?
```

... so slowly ...

```
📭 slack - Slack
📭 spotify - XYconstant - Do It Well (feat. Tom Aspaul)
Action?  [d]elete, [t]erm, [k]ill, [q]uit)?
```

```
📭 slack - Slack
```

Aha, just slack is left, and it's closed its window. It hasn't exited,
though. That would be what we want. Let's *tell* it to quit.

```
Action?  [d]elete, [t]erm, [k]ill, [q]uit)? t
Telling everyone to quit.
```

```
No applications found, exiting.
```

At last, it went away, and we can confidently reboot.
