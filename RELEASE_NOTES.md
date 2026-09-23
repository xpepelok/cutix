cutix 0.0.3: refactoring and bug fixes.

- Smooth playback out of the box: FFmpeg now ships next to the executable, so 1080p recordings
  play at full frame rate in the library, the hover preview and the editor.
- Updates itself: a pill in the title bar offers a new release, verifies it and restarts into it,
  waiting for a running export or upload to finish first.
- Entrance animations for dialogs, screens, cards and toasts; they follow the system's reduced
  motion setting.
- Exports no longer repeat or skip frames on variable frame rate recordings, keep audio in sync at
  59.94 and 23.976 fps, keep their sound when shorter than a second, and refuse to write a file
  with missing media instead of saving a broken one.
- YouTube: scheduled times follow your local clock, cancel works at every step, leftover browsers
  are cleaned up on Windows 11, and signing an account back in no longer disturbs running uploads.
- Many editor fixes: Escape closes the topmost window, shortcuts no longer fire while typing,
  "keep left part" works, project switches can no longer mix up imports, and saves reach disk in
  order.

Unpack the archive for your system and run the binary next to the `lang` and `ffmpeg` folders.
