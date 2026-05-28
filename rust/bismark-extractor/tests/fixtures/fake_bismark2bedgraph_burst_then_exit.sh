#!/bin/sh
# Fake bismark2bedGraph that writes a 128 KiB stderr burst then exits non-zero.
# Guards against the pipe-buffer-full deadlock that would occur if the parent's
# drain thread were spawned AFTER child.wait() (rev 1 I6). With the correct
# drain-before-wait ordering, the parent reads the pipe as it fills.
# 128 KiB = 131072 bytes. Base64 of 96 KiB random data yields ~131 KiB.
head -c 98304 /dev/urandom | base64 >&2
exit 1
