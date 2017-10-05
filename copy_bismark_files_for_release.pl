#!/usr/bin/perl
use warnings;
use strict;
use File::Copy::Recursive qw(fcopy rcopy dircopy fmove rmove dirmove);
use File::Copy "cp";
use File::Spec "catfile";
use File::Spec "splitpath";

my $dir = shift@ARGV;

die "Please provide a directory to copy files to!\n\n" unless ($dir);

unless (-d $dir){
  warn "Specified directory '$dir' doesn't exist. Creating it for you...\n\n";
  mkdir $dir or die "Failed to create directory: $!\n\n";
}

my ($volume, $dist_dir, $this_script) = File::Spec->splitpath(__FILE__);
#my  = abs_path($0);
my @files = ('CHANGELOG.md','bismark','bismark_genome_preparation','bismark_methylation_extractor','bismark2bedGraph','bismark2report','coverage2cytosine','license.txt','Bismark_alignment_modes.pdf','deduplicate_bismark','bam2nuc','bismark2summary','filter_non_conversion','NOMe_filtering');

my @reporting = ('bismark_sitrep.js','bismark_sitrep.tpl','highcharts.js','jquery-3.1.1.min.js');

my @docs = ('make_docs.pl','README.md','Bismark_User_Guide.html');

foreach my $file(@files){ 
    copy_and_warn(File::Spec->catfile($dist_dir, $file));
}
warn "Finished copying normal files\n\n"; sleep(1);

foreach my $file(@reporting){ 
    copy_reports_and_warn(File::Spec->catfile($dist_dir, "bismark_sitrep", $file));
}
warn "Finished copying bismark2report files\n\n"; sleep(1);

foreach my $file(@docs){ 
    copy_docs_and_warn(File::Spec->catfile($dist_dir, "Docs", $file));
}
warn "Finished copying Docs files\n\n"; sleep(1);

sub copy_and_warn{
    my $file = shift;
    warn "Now copying '$file' to $dir\n";
    cp($file,"$dir/") or die "Copy failed: $!";
}

sub copy_reports_and_warn{
    unless (-d "${dir}/bismark_sitrep/"){
	warn "Specified directory '$dir/bismark_sitrep/' doesn't exist. Creating it for you...\n\n";
	mkdir "${dir}/bismark_sitrep/" or die "Failed to create directory '${dir}/bismark_sitrep/': $!\n\n";
    }
    
    my $file = shift;
    warn "Now copying '$file' to $dir/bismark_sitrep/\n";
    cp($file,"$dir/bismark_sitrep/") or die "Copy to '$dir/bismark_sitrep/' failed: $!\n\n";
}

sub copy_docs_and_warn{
    unless (-d "${dir}/Docs/"){
	warn "Specified directory '$dir/Docs/' doesn't exist. Creating it for you...\n\n";
	mkdir "${dir}/Docs/" or die "Failed to create directory '${dir}/Docs/': $!\n\n";
    }
    
    my $file = shift;                                                                                                                            
    warn "Now copying '$file' to $dir/Docs/\n";
    cp($file,"${dir}/Docs/") or die "Copy to '$dir/Docs/' failed: $!\n\n";
}

#######################
### SETTING PERMISSIONS
#######################

@files = ('bismark','bismark_genome_preparation','bismark_methylation_extractor','bismark2bedGraph','bismark2report','coverage2cytosine','deduplicate_bismark','bam2nuc','bismark2summary','filter_non_conversion','NOMe_filtering');

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
system ("tar czvf ${dir}.tar.gz $dir/");
