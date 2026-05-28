#!/bin/sh
# Fake bismark2bedGraph that writes non-UTF-8 bytes to stderr. The
# RealRunner's drain thread MUST NOT panic — rev 1 C5: use
# `read_until(b'\n', &mut Vec<u8>)`, not `read_line(&mut String)`.
printf '\xff\xfe\xfd\xfc binary bytes followed by ascii\n' >&2
printf 'second line is plain ascii\n' >&2
exit 0
