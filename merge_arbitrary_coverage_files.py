#!/usr/bin/env python

# Copyright 2025, Felix Krueger (fkrueger@altoslabs.com)

# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

"""Former name of merge_coverage_files_ARGV.py, kept so existing commands work."""

import os
import sys

script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "merge_coverage_files_ARGV.py")
os.execv(sys.executable, [sys.executable, script, *sys.argv[1:]])
