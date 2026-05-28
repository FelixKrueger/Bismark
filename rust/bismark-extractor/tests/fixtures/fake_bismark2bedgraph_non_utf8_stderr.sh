#!/bin/sh
# Fake bismark2bedGraph that writes non-UTF-8 bytes to stderr. The
# RealRunner's drain thread MUST NOT panic — rev 1 C5: use
# `read_until(b'\n', &mut Vec<u8>)`, not `read_line(&mut String)`.
#
# **POSIX-portable octal escapes** (`\NNN`), NOT hex (`\xNN`). POSIX
# printf does NOT support \xNN — bash extends it, dash (Debian/Ubuntu
# /bin/sh) does not. macOS /bin/sh is bash-in-POSIX-mode so \xNN worked
# locally; Linux CI uses dash which writes the literal 4 chars `\xff`
# instead of byte 0xff, defeating the non-UTF-8 stderr test (rev 3
# CI-fix). Octal mapping: 0xff → \377, 0xfe → \376, 0xfd → \375,
# 0xfc → \374.
printf '\377\376\375\374 binary bytes followed by ascii\n' >&2
printf 'second line is plain ascii\n' >&2
exit 0
