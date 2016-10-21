#!/usr/bin/env perl
use strict;
use warnings;

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

use Text::Markdown 'markdown';

# Load the markdown file specified on the command line
my $input_fn = $ARGV[0];
open my $fh_i, '<', $input_fn or die;
$/ = undef;
my $md = <$fh_i>;
close $fh_i;

# Parse to html
my $html = markdown($md);

# Add ID attributes to headings, collect for ToC
my @toc;
sub cleanid {
    # I leave it as an excercise to the reader to make a more elegant cleanup script.
    # I'm doing this on a train and I'm going to have to get off soon.
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
# use Data::Dumper;
# print Dumper(@toc);

# Load the template file
my $template_fn = 'docs_template.html';
open my $fh_t, '<', $template_fn or die;
$/ = undef;
my $template = <$fh_t>;
close $fh_t;

# Insert the content
$template =~ s/{{ TOC }}/$tocstring/;
$template =~ s/{{ CONTENT }}/$html/;

# Print to output
my $output_fn = $ARGV[1];
open(my $fh_o, '>', $output_fn) or die;
print $fh_o $template;
close $fh_o;