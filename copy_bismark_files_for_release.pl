#!/usr/bin/env perl
use warnings;
use strict;
use File::Copy "cp";

my $dir = shift@ARGV;

die "Please provide a directory to copy files to!\n\n" unless ($dir);

unless (-d $dir){
  warn "Specified directory '$dir' doesn't exist. Creating it for you...\n\n";
  mkdir $dir or die "Failed to create directory: $!\n\n";
}

my @files = ('RELEASE_NOTES.md','bismark','bismark_genome_preparation','bismark_methylation_extractor','bismark2bedGraph','bismark2report','coverage2cytosine','license.txt','./Docs/Bismark_User_Guide.html','./bismark_sitrep/bismark_sitrep.js','./bismark_sitrep/bismark_sitrep.tpl','./bismark_sitrep/highcharts.js','./bismark_sitrep/jquery-3.1.1.min.js','./Docs/make_docs.pl','RRBS_Guide.pdf','deduplicate_bismark','bam2nuc','bismark2summary','filter_non_conversion');

foreach my $file (@files){
  copy_and_warn($file);
}

sub copy_and_warn{
  my $file = shift;
  warn "Now copying '$file' to $dir\n";
  cp($file,"$dir/") or die "Copy failed: $!";

}

@files = ('bismark','bismark_genome_preparation','bismark_methylation_extractor','bismark2bedGraph','bismark2report','coverage2cytosine','deduplicate_bismark','bam2nuc','bismark2summary','filter_non_conversion');

foreach my $file (@files){
  set_permissions($file);
}

sub set_permissions{
  my $file = shift;
  warn "Setting permissions for ${dir}/$file\n";
  chmod 0755, "${dir}/$file";
}


### Taring up the folder
$dir =~ s/\/$//;
warn "Tar command:\ntar czvf ${dir}.tar.gz $dir\n\n";
sleep(3);
system ("tar czvf ${dir}.tar.gz $dir/");
