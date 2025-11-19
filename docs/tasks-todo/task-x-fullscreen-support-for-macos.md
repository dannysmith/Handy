# Task: Fullscreen support for macOS

https://github.com/dannysmith/Handy/pull/1

Handy shows no visual feedback when transcription is active in macOS fullscreen apps, unlike alternatives (Whispr Flow, Voiceink etc). Related issues:

- https://github.com/cjpais/Handy/issues/277
- https://github.com/cjpais/Handy/issues/278

> [!WARNING]
> Before opening PR to upstream repo, revert dccb55b979a20973e7f96d2253136bccc268dd2a

## Requirements

When transcription is active, the usual overlay at the bottom of the screen should display in macOS full screen mode in exactly the same way it does when not in full screen mode. Also, as a secondary feature, when switching spaces between two non-full screen spaces, when the dictation is active, it should also correctly swap spaces in the same way that other tools do.
