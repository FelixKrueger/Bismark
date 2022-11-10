# Summary report

This script uses Bismark report files of several (up to hundreds of!?) samples in a run folder to generate a graphical summary HTML report as well as a whopping big table (tab-delimited text) with all relevant alignment and methylation statistics which may be used for graphing purposes in R, Excel or the like.

Unless certain BAM files are specified, `bismark2summary` first identifies Bismark BAM files in a folder (they need to use the Bismark naming conventions) and then automatically detects Bismark alignment, deduplication or methylation extractor (splitting) reports based on the input file basename.

If splitting reports are found they overwrite the methylation statistics of the initial alignment report.

!!! abstract "Example report"

    You can see an example [Bismark Summary Report here](http://www.bioinformatics.babraham.ac.uk/projects/bismark/bismark_summary_report.html).

!!! tip

    The Bismark summary report is meant to give you a quick visual overview of the alignment statistics for a large number of samples (tens, hundreds or thousands of samples).

    If you only want to look at a single report please check out the `bismark2report`.

## Usage

```
bismark2summary [options]
```

This command scans the current working directory for different Bismark alignment, deduplication and methylation extraction (splitting) reports to produce a graphical summary HTML report, as well as a data table, for all files in a directory.

### Arguments

- `<BAM filename(s)>` (optional)

If no BAM filenames are specified, the current working directory is scanned for Bismark alignment files and their associated reports.

!!! tip

    Note that the actual files are not needed, just filenames. These are used to deduce the report filenames.

### Options

- `-o/--basename <filename>` (optional)

Basename of the output file. Generate a text file with all relevant extracted values `<basename>.txt`) as well as an HTML report (`<basename>.html`). If not specified explicitly, the basename is `bismark_summary_report`.

- `--title <string>` (optional)

HTML report title. Default: '_Bismark Summary Report_'.

!!! note

    Remember to use quotes if using spaces: `--title "speech marks for text with spaces"`

- `--version`

Displays version information and exits.

- `--help`

Displays this help message and exits.
