#!/usr/bin/env bash
pandoc -c docs.css --toc --toc-depth 2 -fmarkdown-implicit_figures -f markdown_github -t html5 README.md -o Bismark_User_Guide.html
