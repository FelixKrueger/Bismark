#!/usr/bin/perl
use warnings FATAL => 'all';
use strict;
use Cwd;
$|++;
use Getopt::Long;

my $parent_dir = getcwd;
my $total_genome_length = 0; ## we need this later to generate random sequences

my %chromosomes;
my %seqs;
my %seqs_colourspace;

my @DNA_bases = ('A','T','C','G');

my ($sequence_length,$conversion_rate,$number_of_sequences,$error_rate,$number_of_SNPs,$quality,$fixed_length_adapter,$variable_length_adapter,$adapter_dimer,$random,$colourspace,$genome_folder,$non_directional) = process_command_line();

run_sequence_generation ();

sub run_sequence_generation{

  warn "\nSelected general parameters:\n";
  warn "-"x50,"\n";
  warn "sequence length:\t$sequence_length bp\n";
  warn "number of sequences being generated:\t$number_of_sequences\n";
  if ($non_directional){
    warn "Non-directional sequences will be generated, i.e. sequences can originate from any of the four possible bisulfite PCR strands\n";
  }
  if ($colourspace){
    warn "Sequences will be written out as both base space and colour space FastQ files\n";
  }
  warn "\n";

  warn "Possible sources of contamination:\n";
  warn "-"x50,"\n";
  warn "overall error rate:\t$error_rate%\n";

  if ($conversion_rate ==0){
    warn "\nPlease note that the bisulfite conversion rate was selected as:\t$conversion_rate %\n";
    warn "This means that reads will not be converted at all and thus serve as simulated genomic FastQ sequences\n\n";
  }
  else{
    warn "bisulfite conversion rate:\t$conversion_rate%\n";
  }

  if ($number_of_SNPs > 0){
    warn "SNPs to be introduced:\t$number_of_SNPs\n";
  }
  if ($number_of_SNPs > 0 or $error_rate == 0){
    warn "default Phred quality value:\t$quality\n";
  }

  if ($fixed_length_adapter){
    warn "Introducing a fixed length adpater contamination into all sequences:\t $fixed_length_adapter bp\n";
  }
  if ($variable_length_adapter){
    warn "Introducing a variable length of adapter sequence into a proportion of all sequences\n";
    warn "assuming a normal distribution of fragment sizes with a mean fragment length (mu) of $variable_length_adapter (user-specified) and a variance (sigma) of 60 (fixed)\n";
  }
  if ($adapter_dimer){
    warn "Introducing $adapter_dimer% of adapter dimers into the simulated dataset\n";
  }
  warn "\n\n";


  if ($random){
    generate_random_sequences ();
  }
  else{
    generate_genomic_sequences (); # DEFAULT
  }

  bisulfite_transform_sequences ();

  if ($non_directional){
    make_non_directional_sequences();
  }

  if ($fixed_length_adapter){
    introduce_fixed_length_adapter_contamination ();
  }
  elsif ($variable_length_adapter){
    introduce_variable_length_adapter_contamination ();
  }

  ### Adapter dimers can theoretically be specified in addition to variable or fixed length adapter sequence contamination
  if($adapter_dimer){
    introduce_adapter_dimers();
  }

  if ($number_of_SNPs > 0){
    introduce_SNPs();
  }

  generate_quality_values ();

  if ($colourspace){
    transform_reads_to_colourspace();
  }

  ### we won't introduce any additional erros if a specific number of SNPs has been specified or the error rate was set to 0%

  if ($number_of_SNPs > 0){
    warn "To gauge the influence of $number_of_SNPs SNPs per sequence on the alignmentment results no additional sequencing errors will be introduced\n\n";
  }
  elsif($error_rate == 0){
    warn "No further sequencing errors will be introduced as the error rate was set to $error_rate%\n\n";
  }
  else{
    introduce_sequencing_errors();
    if ($colourspace){
      introduce_sequencing_errors_colourspace ();
    }
  }

  print_sequences_out_basespace ();

  if ($colourspace){
    print_sequences_out_colourspace ();
  }
}

sub transform_reads_to_colourspace{
  warn "="x117,"\n";
  warn "Now converting all sequences into colour space\n";
  my $count  = 0;
  foreach my $entry (keys %seqs){
    ++$count;
    my $seq = $seqs{$entry}->{sequence};
    my $cs_seq = convert_basespace_to_colourspace($seq);
    my $qual = $seqs{$entry}->{qual};

    $seqs_colourspace{$entry}->{sequence} = $cs_seq;
    $seqs_colourspace{$entry}->{qual} = $qual;
  }
  warn "Successfully converted $count sequences into colour space\n";
  warn "="x117,"\n\n";
}


sub introduce_adapter_dimers{
  warn "="x117,"\n";
  # for the purpose of this contamination it doesn't really matter which sequence it is exactly that causes the contamination
  # so we will just concatenate the Illumina adapter sequence and take a substring equal to the length of the simulated sequences

  ### Taken from the FastQC contaminants list:
  ### Illumina Paired End PCR Primer 2; sequence: CAAGCAGAAGACGGCATACGAGATCGGTCTCGGCATTCCTGCTGAACCGCTCTTCCGATCT
  my $adapter_sequence = 'CAAGCAGAAGACGGCATACGAGATCGGTCTCGGCATTCCTGCTGAACCGCTCTTCCGATCT';
  until (length $adapter_sequence > $sequence_length){
    $adapter_sequence .= $adapter_sequence;
  }

  ### as one would be reading into the adapter from the opposite side we need to reverse complement the sequence
  $adapter_sequence = reverse_complement($adapter_sequence);

  my $adapter_dimer_sequence = substr($adapter_sequence,0,$sequence_length);

  ### determining how many sequences we want to replace with adapter dimers in total
  my $max = int($number_of_sequences*$adapter_dimer/100);
  my $count = 0;

  warn "Now replacing $adapter_dimer% of all simulated sequences with adapter dimers\n";
  # Now cycling through all sequences in the %seqs hash and introducing adapter sequence
  foreach my $entry (keys %seqs){
    ++$count;
    $seqs{$entry}->{sequence} = $adapter_dimer_sequence;

    if ($count >= $max){
      last; # exiting once we have replaced enough sequences with adapater dimers
    }
  }
  warn "Replaced $count sequences with the adapter dimer sequence > $adapter_dimer_sequence < in total\n";
  warn "="x117,"\n\n";
}




sub introduce_fixed_length_adapter_contamination{
  warn "="x117,"\n";
  ### Taken from the FastQC contaminants list:
  ### Illumina Paired End PCR Primer 2; sequence: CAAGCAGAAGACGGCATACGAGATCGGTCTCGGCATTCCTGCTGAACCGCTCTTCCGATCT
  my $adapter_sequence = 'CAAGCAGAAGACGGCATACGAGATCGGTCTCGGCATTCCTGCTGAACCGCTCTTCCGATCT';
  until (length $adapter_sequence > $sequence_length){
    $adapter_sequence .= $adapter_sequence;
  }
  ### as one would be reading into the adapter from the opposite side we need to reverse complement the sequence
  $adapter_sequence = reverse_complement($adapter_sequence);

  ### we'll generate a fixed length adapter contamination for each sequence with the length $fixed_length_adapter which was set
  ### with the option --fixed_length_adpater <int>

  warn "Now introducing $fixed_length_adapter bp of Illumina adapter sequence at the 3' end of each sequence\n";
  my $count = 0;
  my $sub_sequence = substr($adapter_sequence,0,$fixed_length_adapter);

  # Now cycling through all sequences in the %seqs hash and introducing adapter sequence
  foreach my $entry (keys %seqs){
    ++$count;
    my $seq = $seqs{$entry}->{sequence};

    ### replacing the last bases of the sequence with the substitution sequence
    substr($seq,-$fixed_length_adapter,$fixed_length_adapter,$sub_sequence);

    ### replacing the old sequence with the new and modified sequence
    $seqs{$entry}->{sequence} = $seq;
  }

  warn "Replaced last $fixed_length_adapter bp of each sequence with the adapter sequence >$sub_sequence<\n";
  warn "="x117,"\n\n";
}




sub introduce_variable_length_adapter_contamination{
  warn "="x117,"\n";
  ### Taken from the FastQC contaminants list:
  ### Illumina Paired End PCR Primer 2; sequence: CAAGCAGAAGACGGCATACGAGATCGGTCTCGGCATTCCTGCTGAACCGCTCTTCCGATCT
  my $adapter_sequence = 'CAAGCAGAAGACGGCATACGAGATCGGTCTCGGCATTCCTGCTGAACCGCTCTTCCGATCT';

  until (length $adapter_sequence > $sequence_length){
    $adapter_sequence .= $adapter_sequence;
  }

  ### as one would be reading into the adapter from the opposite side we need to reverse complement the sequence
  $adapter_sequence = reverse_complement($adapter_sequence);

  ### we'll generate a random length of a adapter contamination for each sequence
  ### To do this we will first simulate insert sizes with a normal distribution (and not a uniform distribution!)
  ### with a user specified mean fragment length (such as 150 or 200 bp)

  my $mu = $variable_length_adapter; # this is the mean of the bell curve
  my $sigma = 60; # this is the variance (height of the bell curve)

  my @gaussian; # will hold random fragment lengths

  warn "Now generated random fragment sizes with a normal distribution (using the Marsaglia polar method)\n";
  warn "Mean fragment size (mu):\t$mu\t(user-specified)\nVariance (sigma):\t$sigma\t(fixed)\n";

  foreach (1..$number_of_sequences){
    my $gauss = gaussian_rand();
    $gauss *= $sigma;
    $gauss = int($gauss+$mu);
    push @gaussian,$gauss;
  }

  warn "Generated ",scalar @gaussian," normally distributed random numbers\n\n";

  warn "Now analysing fragment lengths and replacing too short sequences with variable stretches of Illumina adapter sequence\n";
  my $count = 0;
  my $small = 0;

  # Now cycling through all sequences in the %seqs hash and introducing adapter sequence if needed
  foreach my $entry (keys %seqs){
    my $fragment = shift @gaussian;
    if ($fragment < 0){
      $fragment = 0;
    }
    unless (defined $fragment){
      warn "Exiting for fragment $fragment\n";
      last;
    }

    ++$count;
    if ($fragment < $sequence_length){
      ++$small;
      my $seq = $seqs{$entry}->{sequence};
      my $sub_length = $sequence_length-$fragment;
      my $sub_sequence = substr($adapter_sequence,0,$sub_length);

      ### replacing the last bases of the sequence with the substitution sequence
      substr($seq,-$sub_length,$sub_length,$sub_sequence);
      if (length$seq < $sequence_length){
	warn "The sequence is now only  ",length($seq)," bp long! $seq\n";
      }

      ### replacing the old sequence with the new and modified sequence
      $seqs{$entry}->{sequence} = $seq;

    }

  }

  my $percent = sprintf ("%.1f",$small*100/$count);

  warn "$count elements analysed in total\n";
  warn "$small elements were smaller than the sequence length $sequence_length ($percent%) and had some adapter sequence introduced\n";
  warn "="x117,"\n\n";
}




sub gaussian_rand {
  ## this subroutine generates 2 independent random numbers with a uniform distribution and transforms them into
  ## 2 independent random numbers with a normal distribution

  my ($u1, $u2);  # uniformly distributed random numbers
  my $w;          # variance, then a weight
  my ($g1, $g2);  # gaussian-distributed numbers

  do {
    $u1 = 2 * rand() - 1;
    $u2 = 2 * rand() - 1;
    $w = $u1*$u1 + $u2*$u2;
  } while ( $w >= 1 ); ## this is important as it might otherwise produce illegal divisions by 0 in the ln step below

  $w = sqrt( (-2 * log($w))  / $w );
  $g1 = $u1 * $w;
  $g2 = $u2 * $w;

  # return both if wanted, else just one
  return $g1;
}




sub print_sequences_out_basespace{
  warn "="x117,"\n";
  warn "Printing sequences in base space out to file\n>>> simulated.fastq <<<\n";
  open (FASTQ,'>','simulated.fastq') or die $!;
  foreach my $entry (sort {$a<=>$b} keys %seqs){
    print FASTQ "@",$entry,"\n";
    print FASTQ "$seqs{$entry}->{sequence}\n";
    print FASTQ "+",$entry,"\n";
    print FASTQ "$seqs{$entry}->{qual}\n";
  }
  warn "Printing out sequences in base space completed\n";
  close FASTQ or die "Unable to close filehandle: $!";
  warn "="x117,"\n\n";
}



sub print_sequences_out_colourspace{
  warn "="x117,"\n";
  warn "Printing sequences in colour space out to file\n>>> simulated_cs.fastq <<<\n";
  open (COLOUR,'>','simulated_cs.fastq') or die $!;
  foreach my $entry (sort {$a<=>$b} keys %seqs_colourspace){
    print COLOUR "@",$entry,"\n";
    print COLOUR "$seqs_colourspace{$entry}->{sequence}\n";
    print COLOUR "+",$entry,"\n";
    print COLOUR "$seqs_colourspace{$entry}->{qual}\n";
  }
  warn "Printing out sequences in colour space completed\n";
  close COLOUR or die "Unable to close filehandle: $!";
  warn "="x117,"\n\n";
}



sub bisulfite_transform_sequences{
  warn "="x117,"\n";
  warn "Now starting bisulfite conversion with a conversion rate of $conversion_rate%\n";
  sleep (2);

  my $total_C_count = 0;
  my $converted_C_count = 0;

  foreach my $entry (keys %seqs){
    my $seq = $seqs{$entry}->{sequence};

    my @bases = split (//,$seq);

    foreach my $base (@bases){

      # only going to change Cs
      if ($base eq 'C'){
	++$total_C_count;
	### converting each C with an individual conversion rate (set globally)
	my $random = int(rand(101));
	
	if ($random <= $conversion_rate){
	  ++$converted_C_count;
	  $base = 'T';
	}

      }

    }

    my $bisulfite_converted_seq = join ("",@bases);
    $seqs{$entry}->{sequence} = $bisulfite_converted_seq;

  }
  my $percentage = sprintf ("%.2f",$converted_C_count*100/$total_C_count);
  warn "Total Cs analysed: $total_C_count;\tConverted Cs: $converted_C_count ($percentage%)\n";
  warn "Bisulfite conversion successfully completed\n";
  warn "="x117,"\n\n";
}



sub make_non_directional_sequences{
  warn "="x117,"\n";
  warn "Now starting to transform bisulfite converted sequences into non-directional reads. All four possible strands will occur with roughly the same likelyhood\n";
  sleep (2);

  my $rc = 0;
  my $count = 0;
  foreach my $entry (keys %seqs){
    ++$count;
    my $seq = $seqs{$entry}->{sequence};

    ### converting each C with an individual conversion rate (set globally)
    my $random = int(rand(100)+1);
	
    if ($random <= 50){
      $seq = reverse_complement($seq);
      ++$rc;
    }

    # reassigning the sequence to %seqs
    $seqs{$entry}->{sequence} = $seq;
  }
  my $percentage = sprintf ("%.2f",$rc*100/$count);
  warn "Total sequences analysed: $count;\tSequences reverse complemented: $rc ($percentage%)\n";
  warn "Introducing non-directionality completed\n";
  warn "="x117,"\n\n";
}



sub introduce_SNPs{
  warn "="x117,"\n";
  warn "Now starting to introduce $number_of_SNPs SNPs per read\n";
  my $total = 0;
  foreach my $entry (sort {$a<=>$b} keys %seqs){
    my $seq = $seqs{$entry}->{sequence};

    my @bases = split (//,$seq);

    ### first determining the positions at which a SNP will be introduced
    my %snps;
    my $SNP_position_count = 0;

    while ($SNP_position_count < $number_of_SNPs){
      my $random= int(rand(length($seq))); # this will be a number between 0 and the read length-1, which we can use directly as index positions
      unless (exists $snps{$random}){
	$snps{$random} = 1;
	++$SNP_position_count;
      }
    }

    # SNPs will be introduced at the positions stored in %snps
    foreach my $position (sort {$a<=>$b} keys %snps){
      my $random = int(rand(3)+1); # will generate a random number between 1 and 3 which we will add to the number index of the number in the @DNA_bases array
      my $base_to_be_substituted = $bases[$position];

      if ($base_to_be_substituted eq 'A'){
	$random += 0;
      }
      elsif ($base_to_be_substituted eq 'T'){
	$random += 1;
      }
      elsif ($base_to_be_substituted eq 'C'){
	$random += 2;
      }
      elsif ($base_to_be_substituted eq 'G'){
	$random += 3;
      }
      else{
	die "base was $base_to_be_substituted\n";
      }
      $random %= 4;

      $bases[$position] = $DNA_bases[$random];
      ++$total;
    }

    my $substituted_sequence = join ("",@bases);
    # print "$seq\n$substituted_sequence\n\n";
    $seqs{$entry}->{sequence} = $substituted_sequence;
  }
  warn "Introducing SNPs successfully completed ($total in total)\n";
  warn "="x117,"\n\n";
}




sub introduce_sequencing_errors{
  warn "="x117,"\n";
  warn "Now starting to introduce sequencing errors according to the error rate encoded by each base's Phred score\n";

  my $total_base_count = 0;
  my $total_errors_introduced = 0;
  my $count = 0;

  foreach my $entry (keys %seqs){
    ++$count;

    my @bases = split (//,$seqs{$entry}->{sequence});
    my @quals = split (//,$seqs{$entry}->{qual});

    unless(scalar@bases == scalar @quals){
      die "The sequence lenght (",scalar@bases,") and length of the quality string (",scalar@quals,") were different which mustn't happen!\n\n";
    }

    foreach my $index (0..$#quals){
      ++$total_base_count;
      my $phred_score = convert_quality_string_into_phred_score ($quals[$index]);
      my $error_rate = convert_phred_score_into_error_probability ($phred_score);

      my $random  = int(rand(10000)+1);
      $random /= 10000;

      if ($random < $error_rate){

	$random = int(rand(3)+1); # will generate a random number between 1 and 3 which we will add to the number index of the number in the @DNA_bases array

	if ($bases[$index] eq 'A'){
	  $random += 0;
	}
	elsif ($bases[$index] eq 'T'){
	  $random += 1;
	}
	elsif ($bases[$index] eq 'C'){
	  $random += 2;
	}
	elsif ($bases[$index] eq 'G'){
	  $random += 3;
	}
	else{
	  die "base was $bases[$index]\n";
	}
	$random %= 4;

	# warn "replacing $bases[$index] with $DNA_bases[$random]\n";
	$bases[$index] = $DNA_bases[$random];

       	++$total_errors_introduced;
      }
    }
    my $substituted_sequence = join ("",@bases);
    # print "$seqs{$entry}->{sequence}\n$substituted_sequence\n$seqs{$entry}->{qual}\n\n";
    $seqs{$entry}->{sequence} = $substituted_sequence;
  }

  my $percentage = sprintf ("%.2f",($total_errors_introduced/$total_base_count*100));
  warn "Sequences analysed in total:\t$count\nbp analysed in total:\t$total_base_count\nRandom sequencing errors introduced in total:\t$total_errors_introduced (percentage: $percentage)\n";
  warn "="x117,"\n\n";
}



sub introduce_sequencing_errors_colourspace{
  warn "="x117,"\n";
  warn "Now starting to introduce sequencing errors into the colour space data according to the error rate encoded by each base's Phred score\n";
  my @colourspace_transitions = qw(0 1 2 3);

  my $total_base_count = 0;
  my $total_errors_introduced = 0;
  my $count = 0;

  foreach my $entry (keys %seqs_colourspace){
    ++$count;

    my @bases = split (//,$seqs_colourspace{$entry}->{sequence});
    my @quals = split (//,$seqs_colourspace{$entry}->{qual});

    foreach my $index (0..$#quals){
      ++$total_base_count;
      my $phred_score = convert_quality_string_into_phred_score ($quals[$index]);
      my $error_rate = convert_phred_score_into_error_probability ($phred_score);

      my $random  = int(rand(10000)+1);
      $random /= 10000;

      if ($random < $error_rate){

	$random = int(rand(3)+1); # will generate a random number between 1 and 3 which we will add to the number index of the number in the @colourspace_transitions array

	### in the special case that the index is 0 and we need to introduce a sequencing error we need to flip a base and not a colourspace transition
	if ($index == 0){
	  if ($bases[$index] eq 'A'){
	    $random += 0;
	  }
	  elsif ($bases[$index] eq 'T'){
	    $random += 1;
	  }
	  elsif ($bases[$index] eq 'C'){
	    $random += 2;
	  }
	  elsif ($bases[$index] eq 'G'){
	    $random += 3;
	  }
	  else{
	    die "Starting base was $bases[$index]\n";
	  }
	  $random %= 4;
	  # warn "replacing $bases[$index] with $DNA_bases[$random]\n";
	  $bases[$index] = $DNA_bases[$random];

	  ++$total_errors_introduced;
	}
	### in all other cases we introduce a single colourspace transition error
	else{
	  if ($bases[$index] eq '0'){
	    $random += 0;
	  }
	  elsif ($bases[$index] eq '1'){
	    $random += 1;
	  }
	  elsif ($bases[$index] eq '2'){
	    $random += 2;
	  }
	  elsif ($bases[$index] eq '3'){
	    $random += 3;
	  }
	  else{
	    die "base transition was $bases[$index]\n";
	  }
	  $random %= 4;
	  #  warn "replacing $bases[$index] with $colourspace_transitions[$random]\n";
	  $bases[$index] = $colourspace_transitions[$random];

	  ++$total_errors_introduced;

	}
      }
    }
    my $substituted_sequence = join ("",@bases);
    # print "$seqs_colourspace{$entry}->{sequence}\n$substituted_sequence\n$seqs_colourspace{$entry}->{qual}\n\n";
    $seqs_colourspace{$entry}->{sequence} = $substituted_sequence;
  }

  my $percentage = sprintf ("%.2f",($total_errors_introduced/$total_base_count*100));
  warn "Sequences analysed in total:\t$count\nbp analysed in total:\t$total_base_count\nRandom transition errors introduced into colour space data in total:\t$total_errors_introduced (percentage: $percentage)\n";
  warn "="x117,"\n\n";
}


sub generate_quality_values{
  warn "="x117,"\n";
  my $var;
  my $error_quality;

  if ($error_rate == 0){
    warn "Starting to generate quality values with a constant Phred score of $quality\n";
  }
  else{
    warn "Generating quality values with a user defined decaying per-bp error rate of $error_rate%\n";
    warn "Starting to work out the slope of the error curve\n";
    $var = determine_slope_of_the_error_rate_curve();

    warn "Error rates per bp will be modelled according to the formula:\n";
    warn "default base quality - 0.034286*position[bp] + 0.0009263*(position[bp]**2)) - 0.00001*(position[bp]**3)*$var)\n\n";

    my @quals;

    for my $x (1..$sequence_length){
      my $term1 = 0.034286*$x;
      my $term2 = 0.0009263*($x**2);
      my $term3 = $var*0.00001*($x**3);

      my $decayed_quality = $quality - $term1 + $term2 - $term3;
      if ($decayed_quality < 2){
	$decayed_quality = 2;
      }

      push @quals,$decayed_quality;

    }	

    ### converting the Phred Scale values into ASCII strings (currently Phred33 format)
    ### This will be performed for all error rate models

    foreach my $qual(@quals){
      $qual = convert_phred_score_into_quality_string($qual);
    }

    $error_quality = join ("",@quals);

  }

  foreach my $entry (keys %seqs){
    my $length = length($seqs{$entry}->{sequence});

    my @quals;

    ### if no error rate was specified we will use a constant quality score for all bases ($quality), which was either determined by the user or which is 40 by default
    if ($error_rate == 0){
      foreach (1..$length){
	push @quals,$quality;
      }	

      foreach my $qual(@quals){
	$qual = convert_phred_score_into_quality_string($qual);
      }

      my $no_error_quality = join ("",@quals);
      $seqs{$entry}->{qual} = $no_error_quality;
    }

    ### Otherwise we will assume that the base call quality deteriorates over time. We will apply an exponential decay formula to the standard quality value $quality
    ### which is 40 by default or can be altered by the user

    else{
      $seqs{$entry}->{qual} = $error_quality;
    }
  }

  print "Successfully generated quality values for ",scalar keys %seqs," sequences\n";
  warn "="x117,"\n\n";
}

sub determine_slope_of_the_error_rate_curve{
  my $user_specified_error_rate = $error_rate/100;

  my $lower_limit = 1;  ## we start at 1 because this is a very flat curve
  my $upper_limit = 1000000; ## this is a curve with an extremely sharp drop
  my $old_lower_limit = $lower_limit;
  my $old_upper_limit = $upper_limit;

  my $var;
  my $count = 0;

  while(1){
    $count++;
    #  print "iteration $count\n";
    #  print "\nnew lower limit:\t$lower_limit\n";
    #  print "new upper limit:\t$upper_limit\n\n";

    # determining mean error rate for lower limit
    $var = $lower_limit;
    my $error_rate_lower_limit = get_mean_error_rate($var);
    #  warn "The mean error per basepair for the lower limit was:\t$error_rate_lower_limit\tfor \$var:\t$var\n";

    # determining mean error rate for upper limit
    $var = $upper_limit;
    my $error_rate_upper_limit = get_mean_error_rate($var);;
    #  warn "The mean error per basepair for the upper limit was:\t$error_rate_upper_limit\tfor \$var:\t$var\n";

    # determining mean error rate for the half distance

    my $half_distance = sprintf ("%.4f",($upper_limit-$lower_limit)/2);

    if ($user_specified_error_rate <= $error_rate_upper_limit){
      if ( ($error_rate_upper_limit-$user_specified_error_rate) <= 0.0001){
	$var = $upper_limit;
	last;
      }
      else{
	$old_upper_limit = $upper_limit;
	$old_lower_limit = $lower_limit;
	$upper_limit = $half_distance+$lower_limit;
      }
    }

    elsif ($user_specified_error_rate > $error_rate_upper_limit){
      if ( ($user_specified_error_rate-$error_rate_upper_limit) <= 0.0001 ){
	$var = $upper_limit;
	last;
      }
      else{
	# print "Set upper limit back from $upper_limit to $old_upper_limit\n";
	$upper_limit = $old_upper_limit;
	$half_distance = sprintf ("%.4f",($upper_limit-$lower_limit)/2);
	$lower_limit = $half_distance+$lower_limit;
	# print "Set lower limit from $old_lower_limit to $lower_limit\n";
      }
    }

    else{
      die "what else can there be? $user_specified_error_rate  $half_distance\n";
    }
  }
  return $var;
}


sub get_mean_error_rate{
  my $var = shift;
  #  print "using $var to calculate error rates\n";
  my @errors;

  ### error rates are calculated as means per bp per sequence length
  for my $x (1..$sequence_length){
    # this formula has been modelled from real data
    my $term1 = 0.034286*$x;
    my $term2 = 0.0009263*($x**2);
    my $term3 = 0.00001*$var *($x**3);

    my $decayed_quality = $quality - $term1 + $term2 - $term3;
    if ($decayed_quality < 2){
      $decayed_quality = 2;
    }
    #  print "Phred score: $decayed_quality\t";
    my $error_rate = sprintf("%.4f",convert_phred_score_into_error_probability($decayed_quality));
    # print "$error_rate\n";
    push @errors,$error_rate;
  }
  my $mean_error_rate;

  foreach my $rate(@errors){
    $mean_error_rate +=$rate;
  }
  $mean_error_rate /= scalar@errors;
  # print "mean error rate for \$var $var: $mean_error_rate\n";
  return $mean_error_rate;
}



sub convert_phred_score_into_quality_string{
  my $qual = shift;
  $qual = chr($qual+33);
  return $qual;
}



sub convert_quality_string_into_phred_score{
  my $string = shift;
  my $qual = ord($string)-33;
  return $qual;
}



sub convert_phred_score_into_error_probability{
  my $qual = shift;
  my $error_rate = 10**(-$qual/10);
  return $error_rate;
}



sub generate_genomic_sequences {
  warn "="x117,"\n";
  read_genome_into_memory ();
  warn "Total length of the genome is $total_genome_length bp\n";
  warn "="x117,"\n\n";

  warn "="x117,"\n";
  warn "Now starting to generate $number_of_sequences sequences of length $sequence_length bp\n";

  my $count = 0;
  my $plus = 0;
  my $minus = 0;

  until ($count == $number_of_sequences){
    my $random = int(rand($total_genome_length)+1);

    my $chromosome_length = 0;
    foreach my $chr (sort keys %chromosomes){
      $chromosome_length += length ($chromosomes{$chr});

      if ( ($random+length($sequence_length)) < $chromosome_length){
	# print "chromosome: $chr\t",$chromosome_length-$random,"\t";
	my $seq = substr ($chromosomes{$chr},$chromosome_length-$random,$sequence_length);

	# if the sequence contains any N's we are generating another random number without any Ns
	last if ($seq =~ /n/i);
	last if (length$seq != $sequence_length);

	# otherwise we randomly choose either a forward or reverse sequence and then print it out
	my $strand = int(rand(2)); # will produce either 0 or 1 with a 50:50 chance
	
	if ($strand == 0){
	  ++$minus;
	  $seq = reverse_complement($seq);
	}	
	else{
	  ++$plus;
	}
	
	++$count;
	$seqs{$count}->{sequence} = $seq;
	last; # exiting once we printed a sequence
      }
    }
  }
  warn "Sequences were successfully generated (+ strand: $plus\t - strand: $minus)\n";
  warn "="x117,"\n\n";
}



sub generate_random_sequences {
  warn "="x117,"\n";
  warn "Now starting to generate $number_of_sequences sequences of $sequence_length bp with totally random sequences\n";

  my $count = 0;
  my $plus = 0;
  my $minus = 0;

  until ($count == $number_of_sequences){

    my @seq;

    for (1..$sequence_length){
      my $random = int(rand(4));
      push @seq, $DNA_bases[$random];
    }
    my $seq = join ("",@seq);

    ++$count;
    $seqs{$count}->{sequence} = $seq;
  }
  warn "Generated $count random sequences in total\n";
  warn "="x117,"\n\n";
}

sub read_genome_into_memory{

  ## reading in and storing the specified genome in the %chromosomes hash
  chdir ($genome_folder) or die "Can't move to $genome_folder: $!";
  print "Now reading in and storing sequence information of the genome specified in: $genome_folder\n\n";

  my @chromosome_filenames =  <*.fa>;

  ### if there aren't any genomic files with the extension .fa we will look for files with the extension .fasta
  unless (@chromosome_filenames){
    @chromosome_filenames =  <*.fasta>;
  }
  unless (@chromosome_filenames){
    die "The specified genome folder $genome_folder does not contain any sequence files in FastA format (with .fa or .fasta file extensions)\n";
  }

  foreach my $chromosome_filename (@chromosome_filenames){

    # skipping the tophat entire mouse genome fasta file
    next if ($chromosome_filename eq 'Mus_musculus.NCBIM37.fa');

    open (CHR_IN,$chromosome_filename) or die "Failed to read from sequence file $chromosome_filename $!\n";
    ### first line needs to be a fastA header
    my $first_line = <CHR_IN>;
    chomp $first_line;

    ### Extracting chromosome name from the FastA header
    my $chromosome_name = extract_chromosome_name($first_line);
	
    my $sequence;
    while (<CHR_IN>){
      chomp;
      if ($_ =~ /^>/){
	### storing the previous chromosome in the %chromosomes hash, only relevant for Multi-Fasta-Files (MFA)
	if (exists $chromosomes{$chromosome_name}){
	  print "chr $chromosome_name (",length $sequence ," bp)\n";
	  die "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name!\n";
	}
	else {
	  if (length($sequence) == 0){
	    warn "Chromosome $chromosome_name in the multi-fasta file $chromosome_filename did not contain any sequence information!\n";
	  }
	  print "chr $chromosome_name (",length $sequence ," bp)\n";
	  $total_genome_length += length $sequence;
	  $chromosomes{$chromosome_name} = $sequence;
	}
	### resetting the sequence variable
	$sequence = '';
	### setting new chromosome name
	$chromosome_name = extract_chromosome_name($_);
      }
      else{
	$sequence .= uc$_;
      }
    }

    if (exists $chromosomes{$chromosome_name}){
      print "chr $chromosome_name (",length $sequence ," bp)\t";
      die "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name.\n";
    }
    else{
      if (length($sequence) == 0){
	warn "Chromosome $chromosome_name in the file $chromosome_filename did not contain any sequence information!\n";
      }
      print "chr $chromosome_name (",length $sequence ," bp)\n";
      $chromosomes{$chromosome_name} = $sequence;
      $total_genome_length += length $sequence;
    }
  }
  print "\n";
  chdir $parent_dir or die "Failed to move to directory $parent_dir\n";
}



sub extract_chromosome_name {
  ## Bowtie seems to extract the first string after the inition > in the FASTA file, so we are doing this as well
  my $fasta_header = shift;
  if ($fasta_header =~ s/^>//){
    my ($chromosome_name) = split (/\s+/,$fasta_header);
    return $chromosome_name;
  }
  else{
    die "The specified chromosome ($fasta_header) file doesn't seem to be in FASTA format as required!\n";
  }
}



sub reverse_complement{
  my $sequence = shift;
  $sequence =~ tr/CATG/GTAC/;
  my $rev_sequence = reverse($sequence);
  return $rev_sequence;
}



sub convert_basespace_to_colourspace{
  my $seq = shift;
  my @seq = split (//,$seq);

  my @cspace;

  my $first_base;
  my $second_base;

  foreach my $index (0..$#seq){

    if ($index == 0) {
      push @cspace, $seq[$index];
      $first_base = $seq[$index];
      next;
    }

    # from base 2 onwards
    $second_base = $seq[$index];

    if ($first_base eq 'A'){
      if ($second_base eq 'A'){
	push @cspace, 0;
      }
      if ($second_base eq 'C'){	
	push @cspace, 1;
      }
      if ($second_base eq 'G'){
	push @cspace, 2;
      }
      if ($second_base eq 'T'){
	push @cspace, 3;
      }
    }

    if ($first_base eq 'C'){
      if ($second_base eq 'A'){
	push @cspace, 1;
      }
      if ($second_base eq 'C'){	
	push @cspace, 0;
      }
      if ($second_base eq 'G'){
	push @cspace, 3;
      }
      if ($second_base eq 'T'){
	push @cspace, 2;
      }
    }

    if ($first_base eq 'G'){
      if ($second_base eq 'A'){
	push @cspace, 2;
      }
      if ($second_base eq 'C'){	
	push @cspace, 3;
      }
      if ($second_base eq 'G'){
	push @cspace, 0;
      }
      if ($second_base eq 'T'){
	push @cspace, 1;
      }
    }

    if ($first_base eq 'T'){
      if ($second_base eq 'A'){
	push @cspace, 3;
      }
      if ($second_base eq 'C'){	
	push @cspace, 2;
      }
      if ($second_base eq 'G'){
	push @cspace, 1;
      }
      if ($second_base eq 'T'){
	push @cspace, 0;
      }
    }

    $first_base = $second_base;

  }
  my $cspace_read = join ("",@cspace);
  # warn "$seq\n$cspace_read\n\n";
  return $cspace_read;
}



sub process_command_line{
  my $help;
  my $length;
  my $conversion_rate;
  my $snps;
  my $error_rate;
  my $random;
  my $genome_folder;
  my $fixed_length_adapter; ### replaces <int> bp at the end of each sequence
  my $variable_length_adapter;  ### replaces a variable amount of sequence at the end of some sequences
  my $adapter_dimer;  ### introduces <int> % of adapter dimers into the sequence simulation file
  my $number_of_seqs;
  my $quality;
  my $colourspace;
  my $non_directional;

  my $command_line = GetOptions ('help|man' => \$help,
				 'l|length=i' => \$length,
				 'cr|conversion_rate=i' => \$conversion_rate,
				 'variable_length_adapter=i' => \$variable_length_adapter,
				 'fixed_length_adapter=i' => \$fixed_length_adapter,
				 'adapter_dimer=i' => \$adapter_dimer,
				 'e|error_rate=f' => \$error_rate,
				 'n|number_of_seqs=i' => \$number_of_seqs,
				 's|snps=i' => \$snps,
				 'q|quality=i' => \$quality,
				 'random' => \$random,
				 'colourspace' => \$colourspace,
				 'genome_folder' => \$genome_folder,
				 'non_directional' => \$non_directional,
				);


  ### EXIT ON ERROR if there were errors with any of the supplied options
  unless ($command_line){
    die "Please respecify command line options\n";
  }

  ### HELPFILE
  if ($help){
    print_helpfile();
    exit;
  }


  ##################################
  ### PROCESSING OPTIONS
  if (defined $length){
    if ($length == 0){
      warn "A sequence of 0 bp length? ... Please respecify!\n";
      print_helpfile();
      exit;
    }
    if ($length < 0 or $length > 300){
      warn "A sequence shorter than 0 bp or longer than 300bp? ... Please respecify!\n";
      print_helpfile();
      exit;
    }
  }
  else{
    warn "Please specify a sequence length!\n";
    sleep (2);
    print_helpfile();
    exit;
  }

  if (defined $conversion_rate){
    unless ($conversion_rate >= 0 and $conversion_rate <= 100){
      die "Please specify the BS conversion rate as integer value between 1 and 100\n";
    }
  }
  else{
    ## otherwise we assume a 100 % conversion rate
    $conversion_rate = "100";
  }

  ### NUMBER OF SEQUENCES TO BE SIMULATED
  if (defined $number_of_seqs){
    unless ($number_of_seqs > 0){
      die "Please select a sensible number of sequences to be generated (any positive integer, default = 1,000,000)\n";
    }
  }
  else {
    $number_of_seqs = 1000000;
  }

  ### ERROR RATE
  if (defined $error_rate){
    unless ($error_rate >= 0 and $error_rate <= 60){
      die "Please select an error rate between 0 and 60(%)!\n";
    }
  }
  else {
    $error_rate = 0;
  }

  ### SNPs TO BE INTRODUCED
  # if the user wants a certain amount of SNPs we are assuming the same Phred qualities for all bases, including the SNP positions

  if (defined $snps){

    die "The number of SNPs to be introduced can't be higher than the read length (and needs to be greater than 0)!\n" unless ($snps > 0 and $snps <= $length);

    if ($error_rate > 0){
      warn "Specifying an error rate and SNPs at the same time is not compatible. The error rate will be set to 0% and $snps SNPs will be introduced into the read.\nAll positions in the read will have the same basecall quality (which can be set with -q/--quality, 40 by default).\n\n";
    }

    $error_rate = 0;

  }

  else{
    $snps = 0;
  }

  ### RANDOM SEQUENCES INSTEAD OF MOUSE GENOME ONES
  unless ($random){
    $random = 0;
  }


  ### THE BASECALL QUALITY FOR READS WITHOUT ERROR RATE CAN BE SET MANUALLY

  if (defined $quality){
    unless ($quality >=2 and $quality <= 40){
      die "The quality values must be in the range of (Phred) 2-40! Please respecify\n";
    }
  }
  else{
    $quality = 40; ### this is the default
  }

  ### ADAPTER CONTAMINATION OPTIONS

  ### FIXED LENGTH ADAPTER
  if ($fixed_length_adapter){
    die "The adapter contamination must be shorter than the sequence length itself (and greater than 0). Please respecify\n" unless ($fixed_length_adapter > 0 and $fixed_length_adapter < $length);
    die "Fixed-length (all sequences) and variable-length (insert-size depenedent) adapter contaminations are mutually exclusive. Please select one of them only\n!" if (defined $variable_length_adapter);
    $variable_length_adapter = 0;
  }

  ### VARIABLE LENGTH ADAPTER
  if ($variable_length_adapter){
    unless ($variable_length_adapter >= 30 and $variable_length_adapter <= 400){
      die "The mean fragment length should be in the range of 30 to 400 (bp). Please respecify!\n\n";
    }
    $fixed_length_adapter = 0;
  }

  ### ADAPTER DIMER
  if (defined $adapter_dimer){
    unless ($adapter_dimer > 0 and $adapter_dimer <= 100){
      die "Adapter dimer contamination rate was selected as $adapter_dimer%. Please select something in the range of 1 to 100%!\n\n";
    }
  }
  else{
    $adapter_dimer = 0;
  }

  ### COLOURSPACE
  unless ($colourspace){
    $colourspace = 0;
  }

  ### GENOME FOLDER
  if ($genome_folder){
    unless ($genome_folder =~/\/$/){
      $genome_folder =~ s/$/\//;
    }
    warn "Genome folder was specified as $genome_folder\n";
  }
  else{
    $genome_folder = '/data/public/Genomes/Mouse/NCBIM37/';
    warn "Using the default genome folder /data/public/Genomes/Mouse/NCBIM37/ \n";
  }

  unless ($non_directional){
    $non_directional = 0;
  }

  return ($length,$conversion_rate,$number_of_seqs,$error_rate,$snps,$quality,$fixed_length_adapter,$variable_length_adapter,$adapter_dimer,$random,$colourspace,$genome_folder,$non_directional);
}

sub print_helpfile{
  print << 'HELP';


CONTAMINATIONS:


-s/--snps <int>                   The number of SNPs to be introduced. This value can be anything between 1
                                  and the total sequence length. Default: 0. Introducing SNPs will always
                                  assume an error rate of 0%, the default quality for all bases can be
                                  specified with (-q/--quality).

-e/--error_rate <float>           The error rate in %. This can be anything between 0 and 60%. If the error
                                  rate is selected as 0%, no sequencing errors will be introduced (even though
                                  a Phred score of 40 formally translates into an error rate of 0.01%). The
                                  error rate will be a mean error rate per bp whereby the error curve follows
                                  an exponential decay model. This means that an error rate of 0.1% will
                                  - overall - introduce sequencing errors roughly every 1 in 1000 bases, whereby
                                  the 5' end of a read is much less likely to harbour errors than
                                  bases towards the 3' end.

--adapter_dimer <int>             Include an <int> percentage of adapter dimers into the output file.
                                  We are using the Illumina Paired End PCR Primer 2 as adapter sequence.

--fixed_length_adapter <int>      Replaces the most 3' <int> bp of each read with Illumina adapter
                                  sequence. The adapter sequence is the Illumina Paired End PCR
                                  Primer 2.

--variable_length_adapter <int>   For this contamination we simulate a normal distribution of fragment
                                  sizes for a mean insert size specified as <int> bp and replace a
                                  variable portion at the 3' end of reads with a adapter sequence if the
                                  fragment size is smaller than the read length. A normal distribution
                                  of fragment sizes will be modelled using the specified <int> as mean
                                  (mu) and a variance (sigma) of 60 (this is a fixed value which was
                                  determined empirically).



BASIC ATTRIBUTES:

-l/--length                       The length of all sequences to be generated.

--random                          The sequences will be generated with entirely random base composition
                                  instead extracting real sequences from the mouse genome. This is a much
                                  quicker option for testing purposes.

-n/--number-of_seqs               The number of sequences to be generated by the FastQ simulator. Default:
                                  1000000.

-q/--quality                      The default quality for all positions if error rate is set to 0% or if
                                  SNPs are to be introduced. Default: 40.

-cr/--conversion_rate <int>       Bisulfite conversion rate as <int> %. This value can be anything between
                                  0 (no bisulfite conversion at all, thus standard simulated (genomic) sequences)
                                  and 100% (all cytosines will be converted into thymines).

--colourspace                     Using this option will print out all sequences in colour space as well
                                  as in base space FastQ format. Note that the conversion of base space to
                                  colourspace takes place before any quality values or errors are introduced.

--genome_folder <path/to/folder>  Enter the genome folder you wish to use to extract sequences from. Default:
                                  /data/public/Genomes/Mouse/NCBIM37/ .

--non-directional                 The reads can orignate from any of the four possible strands produced by
                                  bisulfite conversion and PCR amplification. Default: OFF.

HELP
}
