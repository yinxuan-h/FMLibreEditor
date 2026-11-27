# Compatibility

Which versions of Football Manager 27 FMLibreEditor has been checked with, and what waits on
some of them.

## Systems and storefronts

| System | FM27 from | |
| --- | --- | --- |
| Windows 10 or 11 | Xbox / Game Pass | Checked with the smoke tests before every release |
| Windows 10 or 11 | Steam | Checked with the smoke tests before every release |
| macOS, Apple silicon | Steam | Supported |
| macOS, Intel | Steam | Players, staff and clubs; see [below](#macos-steam) |

## Versions of the game

Every update of FM27 moves its data, so the editor keeps a profile for each version it was
checked with (`profiles/fm27/`). On a version listed below it reads and edits players, staff
and clubs. On a version that is not listed it opens read-only and says that the version is
new to it; a release that knows the version usually follows within days.

Four features need a check of their own on each version, against what the game's screens
show. Where nobody has made that check yet, the table has a dash, and the app says in place
why there is nothing to show:

- **Prize money:** what a club's league pays, in the club's finances.
- **Sponsorship deals:** a club's deals, in its finances (read only).
- **Squad rules:** the Competitions tab.
- **National teams:** the Clubs tab's National teams switch.

The newest version is first. The tables describe the code on `main`; a version of the game
added since the [latest release](https://github.com/tozmoz/FMLibreEditor/releases/latest)
works from the release after it.

### Windows, Xbox / Game Pass

| Version | Prize money | Sponsorship deals | Squad rules | National teams |
| --- | --- | --- | --- | --- |
| 0.9.2562.0 | yes | yes | yes | yes |
| 0.9.2524.0 | yes | yes | yes | yes |
| 0.9.2435.0 | yes | – | – | yes |
| 0.9.2409.0 | yes | – | – | – |
| 0.9.2355.0 | – | – | – | – |

### Windows, Steam

| Build | Prize money | Sponsorship deals | Squad rules | National teams |
| --- | --- | --- | --- | --- |
| 26797085 | yes | yes | yes | yes |
| 26620374 | yes | – | – | yes |
| 26577676 | yes | – | – | yes |
| 26385295 | – | – | – | – |
| 26340721 | yes | yes | – | – |
| 26227640 | – | – | – | – |

### Windows, Epic Games Store

| Version | Prize money | Sponsorship deals | Squad rules | National teams |
| --- | --- | --- | --- | --- |
| 27.1.0 | yes | – | – | yes |

The Epic version is the same code as Steam build 26620374, so its profile keeps what that
build's keeps. Nobody has checked those places in a running game on Epic.

### macOS, Steam

Apple silicon:

| Build | Prize money | Sponsorship deals | Squad rules | National teams |
| --- | --- | --- | --- | --- |
| 26797085 | yes | yes | yes | yes |
| 26620374 | yes | yes | yes | yes |
| 26577676 | – | – | – | – |
| 26385295 | yes | yes | – | yes |
| 26340721 | yes | – | – | – |
| 26227640 | – | – | – | – |

Intel Macs run the other half of the same six builds. The editor knows where each keeps its
players, staff and clubs; the later ones were worked out from the game's file, not in a
running game. None of the four features above is placed on an Intel build yet.

## What depends on the system

- **Arranging a transfer or a loan for the game to make** works on Windows and on Macs with
  Apple silicon. It does not work on Intel Macs, nor where the game runs under Rosetta.
- **On macOS** the game has to be started from Steam with the bridge set up; see
  [INSTALL.md](INSTALL.md#macos).

## Filling a gap

A dash is a check nobody has made, and you can make it with your copy of the game:
[DEVELOPING.md](DEVELOPING.md#on-the-day-of-an-update) says what to compare with which screen
of the game, and which line then goes into the version's profile. The same section covers a
version of the game that is not listed here at all.
