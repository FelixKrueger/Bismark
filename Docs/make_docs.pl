#!/usr/bin/env perl
use strict;
use warnings;
use Text::Markdown 'markdown';

## This program is free software: you can redistribute it and/or modify
## it under the terms of the GNU General Public License as published by
## the Free Software Foundation, either version 3 of the License, or
## (at your option) any later version.

## This program is distributed in the hope that it will be useful,
## but WITHOUT ANY WARRANTY; without even the implied warranty of
## MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
## GNU General Public License for more details.

## You should have received a copy of the GNU General Public License
## along with this program. If not, see <http://www.gnu.org/licenses/>.

### This script takes an input Markdown file, typically the Bismark User Guide
### and converts it to HTML using the Text::Markdown perl package.
### It inserts this into a static template and builds a side navigation
### / table of contents automatically, based on level 1/2 headings.

# Command line arguments
my ($input_fn, $output_fn) = @ARGV;
if (not defined $output_fn) {
  die "Usage: make_docs.pl input.md output.html\n";
}

# Load the markdown file specified on the command line
open my $fh_i, '<', $input_fn or die;
$/ = undef;
my $md = <$fh_i>;
close $fh_i;

# Parse to html
my $html = markdown($md);

# Add ID attributes to headings, collect for ToC
my @toc;
sub cleanid {
    my ($text, $level) = @_;
    (my $id = $text) =~ s/[^A-Za-z]+/-/g;
    $id = lc($id);
    $id =~ s/code//g;
    $id =~ s/-+/-/g;
    $id =~ s/^-+|-+$//g;
    $level =~ s/\D//g;
    push(@toc, {'level' => $level, 'id' => $id, 'text' => $text});
    return $id;
}
# This is pretty unstable, don't use elsewhere.
$html =~ s/(<h[0-9])>(.+?)(?=<\/h)/$1.' id="'.cleanid($2, $1).'">'.$2/ge;

# Build the ToC
my $tocstring = '<ul>';
my $toclevel = 1;
my $first = 1;
foreach (@toc){
    next if ($_->{'level'} > 2);
    if($_->{'level'} > $toclevel){
        $tocstring .= '<ul>';
    } elsif($_->{'level'} < $toclevel){
        $tocstring .= '</ul></li>'
    } elsif(!$first) {
        $tocstring .= '</li>';
    }
    $tocstring .= '<li><a href="#'.$_->{'id'}.'">'.$_->{'text'}.'</a>';
    $first = 0;
    $toclevel = $_->{'level'};
}
$tocstring .= '</li></ul>';

my $template = << 'DOCS_TEMPLATE';
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=yes">
  <title>Bismark User Guide</title>
  <style type="text/css">
    body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif,"Apple Color Emoji","Segoe UI Emoji","Segoe UI Symbol";margin:10px;color:#333}
    blockquote{font-style:italic;font-size:.9em;color:#999}
    h1{margin-top:80px;padding-bottom:5px;border-bottom:1px solid #dedede}
    img{max-width:100%}
    pre{max-width:100%;overflow:auto;background-color:#ededed;border-radius:5px;padding:8px}
    code{white-space:pre-wrap;background-color:#f9f9f9;padding:3px 5px;font-family:Consolas,"Liberation Mono",Menlo,Courier,monospace;font-size:90%}
    pre code{background-color:#ededed;white-space:pre}
    hr{margin:30px 0;border:0;border-top:1px solid #dedede;height:0}
    table { border-collapse: collapse; }
    table tbody tr { border-top: 1px solid #ccc; }
    table tbody tr td, table thead tr th { padding: 6px 13px; border: 1px solid #ddd }
    table tbody tr:nth-child(2n) { background-color: #f8f8f8; }
    #header_img{float:right;max-width:30%;margin-top:-60px}
    #TOC::before{content:'Table of Contents';font-weight:700;font-size:2em}
    #TOC{font-size:.9em;background-color:#ededed;border-radius:10px;padding:10px}
    #TOC ul{padding:0;list-style-type:none;margin-bottom:10px}
    #TOC ul ul{margin-left:10px;text-decoration:italic}
    #TOC ul li{margin-top:5px}
    #TOC a{text-decoration:none;color:#333}
    #TOC ul ul a{color:#999}
    #TOC ul ul a code{background-color:transparent}
    @media screen and (min-width: 700px) {
      body{margin:20px 420px 20px 20px}
      #TOC{position:fixed;right:20px;top:20px;bottom:20px;width:350px;max-height:100%;overflow:auto}
    }
    #bismark-bisulfite-mapper{margin-top:0;font-size:6em;font-weight:100;border:none}
  </style>
  <!--[if lt IE 9]><script src="//cdnjs.cloudflare.com/ajax/libs/html5shiv/3.7.3/html5shiv-printshiv.min.js"></script><![endif]-->
</head>
<body>
<nav id="TOC">{{ TOC }}</nav>
{{ CONTENT }}
</body>
</html>
DOCS_TEMPLATE


# Insert the content
$template =~ s/{{ TOC }}/$tocstring/;
$template =~ s/{{ CONTENT }}/$html/;

# Print to output
open(my $fh_o, '>', $output_fn) or die;
print $fh_o $template;
close $fh_o;
