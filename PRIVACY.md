# Privacy

Filegram scans your disk locally. The scan itself never leaves your computer:
no file name, folder name, path or file content is ever sent anywhere.

Filegram can send anonymous usage statistics, which is what this document is
about. Update checks against the GitHub releases API are separate and always
on; they send nothing but the request itself.

## What is sent

Every report carries:

- a random install id, generated on first launch and stored in the config
  directory (`device`); it is not tied to your account, machine or network
- the Filegram version, the operating system name and the CPU architecture
- how the build was installed: `flatpak`, `snap` or `direct`

And, depending on what happened:

| Report | What it carries |
| --- | --- |
| `app_started` | whether this is the first launch after allowing reports |
| `session_ended` | how long the session lasted, how many scans, zoom-ins and go-ups it had — all as ranges |
| `scan_started` | which entry point started it: `home`, `downloads`, `desktop`, `documents`, `disk`, `recent` or `typed` |
| `scan_finished` | how long the scan took, how many files and how much data it covered — all as ranges |
| `scan_cancelled` | how long the scan had run |
| `file_action` | `open`, `reveal` or `trash`, and whether it worked |
| `setting_changed` | the theme or the interface language |
| `update_noticed`, `update_opened` | that a newer release exists, and that its page was opened |
| `panicked` | the source file and line Filegram crashed at |
| `telemetry_disabled` | that reporting was switched off, sent right before it stops |

Counts and sizes are always ranges, never exact values: `10k-100k` files,
`10-100GB`, `5-30s`. An exact figure would be close to unique to one machine;
a range is not.

## What is never sent

Paths, file and folder names, file contents, your user name, your machine name,
and the panic message (paths reach it too easily, so only the source location
is reported).

Nor your location. PostHog resolves the sender's IP into a city, a postcode and
coordinates by default; every report switches that off (`$ip` and
`$geoip_disable`), so no location is kept, not even the country.

Nothing here is an aspiration: the test suite asserts it on every event, and a
new field that carries a raw value fails the build.

## Who receives it

[PostHog](https://posthog.com) (EU region), using a write-only project key.

Timestamps are sent in UTC, to the second.

## Turning it on and off

- **Flatpak and Snap builds** ask on first launch and send nothing until you
  answer.
- **Every other build** reports by default and says so on first launch.

Either way, the chart button in the bottom-left corner of the start screen
turns reporting on and off at any time. The choice is stored in the settings
file next to the theme and language.

## Seeing it for yourself

```sh
filegram --telemetry-dump
```

prints the choice in force, the install id and every report still queued for
sending, without sending or deleting anything.
