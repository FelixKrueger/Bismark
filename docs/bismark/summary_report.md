
# Summary report

This script uses Bismark report files of several (up to hundreds of!?) samples in a run folder to generate a graphical summary HTML report as well as a whopping big table (tab-delimited text) with all relevant alignment and methylation statistics which may be used for graphing purposes in R, Excel or the like. Unless certain BAM files are specified, `bismark2summary` first identifies Bismark BAM files in a folder (they need to use the Bismark naming conventions) and then automatically detects Bismark alignment, deduplication or methylation extractor (splitting) reports based on the input file basename. If splitting reports are found they overwrite the methylation statistics of the initial alignment report.


**USAGE:**
```
bismark2summary [options]
```

This command scans the current working directory for different Bismark alignment, deduplication and methylation extraction (splitting) reports to produce a graphical summary HTML report, as well as a data table, for all files in a directory. Here is a sample [Bismark Summary Report](http://www.bioinformatics.babraham.ac.uk/projects/bismark/bismark_summary_report.html). The Bismark summary report is meant to give you a quick visual overview of the alignment statistics for a large number of samples (tens, hundreds or thousands of samples); if you only want to look at a single report please check out the `bismark2report`.

#### ARGUMENTS:

- BAM file(s)

  Optional. If no BAM files are specified explicitly the current working directory is scanned for Bismark alignment files and their associated reports.

#### OPTIONS:

- `-o/--basename <filename>`

Basename of the output file (optional). Generate a text file with all relevant extracted values '_basename_.txt') as well as an HTML report ('*basename*.html'). If not specified explicitly, the basename is '_bismark\_summary\_report_'.

- `--title <string>`

Optional HTML report title; use `--title "speech marks for text with spaces"`. Default: '*Bismark Summary Report*'.


- `--version`

Displays version information and exits.

- `--help`

Displays this help message and exits.
