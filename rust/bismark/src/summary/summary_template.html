<!DOCTYPE html>
<html>
<head>
	<meta http-equiv="content-type" content="text/html; charset=UTF-8">
	<title>Bismark Project Summary Report - {{page_title}}</title>
	<style type="text/css">
		body {
			font-family: Arial, sans-serif;
			font-size:14px;
			padding:0 20px 20px;
		}
		.container {
			margin:0 auto;
		}
		.header h1,
		.header img {
			float:left;
		}
		.header h1 {
			margin: 20px 0 10px;
		}
		.header img {
			padding: 0 20px 20px 0;
		}
		.subtitle {
			margin-top:120px;
			float:right;
			text-align:right;
		}
		.header_subtitle h3,
		.header_subtitle p {
			margin:0;
		}
		h1 {
			font-size: 3.2em;
		}
		h2 {
			font-size:2.2em;
		}
		h3 {
			font-size:1.4em;
		}
		h2, h3, hr {
			clear:both;
		}
		hr {
			border-top:1px solid #CCC;
			border-bottom:1px solid #F3F3F3;
			border-left:0;
			border-right:0;
			height:0;
		}
		.plot {
			width:100%;
			margin-bottom:0px;
            height: 350px;
		}
        .plot_meth{
            width:100%;
            margin-bottom:0px;
            margin-top: 0px;
            height:200px;
        }
		footer {
			color:#999;
		}
		footer a {
			color:#999;
		}
        .switch_group {
			margin-left:20px;
			display:inline-block;
			line-height: 1em;
		}
		.switch_group button {
			vertical-align:top;
			border: 1px solid #2f7ed8;
			border-left:0;
			background-color: #2f7ed8;
			color:#FFFFFF;
			padding: 8px;
			outline: none;
			cursor: pointer;
			-webkit-transition: background-color 150ms ease-in-out;
			-moz-transition: background-color 150ms ease-in-out;
			-o-transition: background-color 150ms ease-in-out;
			-ms-transition: background-color 150ms ease-in-out;
			transition: background-color 150ms ease-in-out;
		}
		.switch_group button:first-child {
			border-left: 1px solid #2f7ed8;
		}
		.switch_group button.active {
			background-color: #2f7ed8;
			color:#FFFFFF;
		}
	</style>

	<!-- Plotly.js -->
	{{plotly_goes_here}}
 	This will need to be replaced by the plot.ly library itself
 	{{plotly_goes_here}}
	
</head>
<body>


<div class="container">
	<div class="header">
		{{bismark_logo_goes_here}}
		<h1>Bismark Project Overall Summary</h1>
		<div class="subtitle">
			<h3>{{page_title}}</h3>
			<p>Report generated on {{report_timestamp}}</p>
		</div>
	</div>
	<hr>
	<h2>
		Alignment Statistics
		<div class="switch_group">
			<!-- <button id="version1">Number of Reads</button> <button id="version2">Percentages</button></div><div id="myData" style="width: 100%;"></div> -->
	</h2>
		
	<div id="alignmentPercentage" class="plot"><!-- Plotly chart will be drawn inside this DIV --></div>
	<div id="alignmentNumbers" class="plot"><!-- Plotly chart will be drawn inside this DIV --></div>
    <hr>
	<h2>
		Cytosine Methylation
	</h2>
        <div id="methylation_context_CpG" class="plot_meth"></div> 
        <div id="methylation_context_CHG" class="plot_meth"></div>
        <div id="methylation_context_CHH" class="plot_meth"></div>
	<hr>
	
	<!-- ### PLOT.LY CODE ##################################################################################################### -->

    <!-- ### ALIGNMENT SECTION -->

	<script>
		var alignment_plot_colors = ['#f28f43', '#0d233a', '#492970', '#2f7ed8', '#8bbc21'];
		var alignment_plot_colors2 = ['#01665e','#f28f43', '#0d233a', '#492970', '#2f7ed8', '#8bbc21'];	
		var meth_plot_colors = ['#0d233a', '#2f7ed8', '#8bbc21', '#1aadce', '#910000', '#492970'];
		var stacksDivPercentage = document.getElementById("alignmentPercentage");
        var stacksDivNumbers = document.getElementById("alignmentNumbers");
		var num_samples = {{num_samples}};
		
		var traces1 = [
			
			{x: [{{x_values_alignment}}], y: [{{no_seq}}],
				fill: 'tonexty',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				name:'No Genomic Sequence',
				fillcolor: '#f28f43',
                line:{
                    color: '#f28f43',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			<!-- Either deduplicated unique reads or Raw unique reads here (e.g. for RRBS) -->
			{{raw_aligned_reads_section}}
			{x: [{{x_values_alignment}}], y: [{{aligned_seq}}],
				fill: 'tozeroy',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				name:'Raw Aligned Reads',
				fillcolor: '#01665e',
				visible: true,
                line:{
                    color: '#01665e',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			{{raw_aligned_reads_section}}
			{{deduplicated_unique_reads_section}}
			{x: [{{x_values_alignment}}], y: [{{unique_alignments}}],
				name:'Deduplicated Unique Alignments',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				fill: 'tonexty', 
				fillcolor: '#8bbc21',
                line:{
                    color: '#8bbc21',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			{{deduplicated_unique_reads_section}}
			{{duplicated_reads_section}}
			{x: [{{x_values_alignment}}], y: [{{dup_alignments}}],
				name:'Duplicate Alignments',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				fill: 'tonexty', 
				fillcolor: '#2f7ed8',
                line:{
                    color: '#2f7ed8',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			{{duplicated_reads_section}}
			{x: [{{x_values_alignment}}], y: [{{ambig_aligned}}],
				name:'Aligned Ambiguously',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				fill: 'tonexty',
				fillcolor: '#492970',
                line:{
                    color: '#492970',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			{x: [{{x_values_alignment}}], y: [{{not_aligned}}],
				fill: 'tonexty',
				name:'Did Not Align',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				fillcolor: '#0d233a',
                line:{
                    color: '#0d233a',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
		];
	
		<!-- Percentage Plots -->
        var traces2 = [
            
			
			{x: [{{x_values_alignment}}], y: [{{p_no_seq_replace}}],
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                fill: 'tozeroy',
                name:'No Genomic Sequence',
				hoveron: 'points+fills',
                fillcolor: '#f28f43',
                line:{
                    color: '#f28f43',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },
			<!-- Either deduplicated unique reads or Raw unique reads here (e.g. for RRBS) -->
			{{deduplicated_unique_reads_percentage_section}}
			{x: [{{x_values_alignment}}], y: [{{p_deduplicated_unique_alignments}}],
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				hoveron: 'points+fills',
				name:'Deduplicated Unique Alignments',
				fill: 'tonexty', 
				fillcolor: '#8bbc21',
                line:{
                    color: '#8bbc21',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			{{deduplicated_unique_reads_percentage_section}}
			{{duplicated_reads_percentage_section}}
			{x: [{{x_values_alignment}}], y: [{{p_duplicated_alignments}}],
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				name:'Duplicate Alignments', 
				hoveron: 'points+fills',
				fill: 'tonexty', 
				fillcolor: '#2f7ed8',
                line:{
                    color: '#2f7ed8',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
			},
			{{duplicated_reads_percentage_section}}
			{{raw_unique_reads_percentage_section}}
			{x: [{{x_values_alignment}}], y: [{{p_aligned_replace}}],
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
				hoveron: 'points+fills',
                fill: 'tonexty',
                name:'Raw Aligned Reads',
                fillcolor: '#01665e',
                visible: true,
                line:{
                    color: '#01665e',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },
			{{raw_unique_reads_percentage_section}}
			
			{x: [{{x_values_alignment}}], y: [{{p_ambig_replace}}],
				hovertext: [{{filenames_replace}}],
				text: [{{filenames_replace}}],
                name:'Aligned Ambiguously',
				hoveron: 'points+fills',
                fill: 'tonexty',
                fillcolor: '#492970',
                line:{
                    color: '#492970',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },			
            {x: [{{x_values_alignment}}], y: [{{p_unal_replace}}],
				hovertext: [{{filenames_replace}}],
				text: [{{filenames_replace}}],
				hoveron: 'points+fills',
                fill: 'tonexty',
                name:'Did Not Align',
                fillcolor: '#0d233a',
                line:{
                    color: '#0d233a',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },

        ];

		function stackedArea(traces) {
			var i, j;
			for(i=0; i<traces.length; i++) {
				traces[i].text = [];
				traces[i].hoverinfo = 'all';
				for(j=0; j<(traces[i]['y'].length); j++) {
					traces[i].text.push(traces[i]['y'][j].toFixed(0)+" "+ traces[i].name);
				}
			}
			for(i=1; i<traces.length; i++) {
				for(j=0; j<(Math.min(traces[i]['y'].length, traces[i-1]['y'].length)); j++) {
					traces[i]['y'][j] += traces[i-1]['y'][j];
				}
			}
			return traces;
		}
		
		var layoutNumbers = {
		
			hoverlabel:{
				namelength: -1,
			},
			xaxis: {
				titlefont: {
					size: 14,
					color: '#4d759e',
				},
				showgrid: false,
                title: '<b>Sample [Hover to identify Sample and Numbers]</b>',
			},
			margin: {
                l: 50,
                r: 20,
                b: 40,
                t: 0, 
                pad: 0,           
            },
			yaxis: {
				title: '<b># Reads</b>',
				titlefont: {
					size: 14,
					color: '#4d759e',
				}
			},
			legend:{
				x: 0.4,
				y: 1.08,
				orientation: 'h',
			}				
		}
        var layoutPercentage = {
			hoverlabel:{
				namelength: -1,
			},
            xaxis: {
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                },
                showgrid: false,
            },
             margin: {
                l: 50,
                r: 20,
                b: 40,
                t: 0,
                pad: 0,
            },
            yaxis: {
                title: '<b>% of all Reads</b>',
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                },
				hoverformat: '.2f',
            },
            legend:{
                x: 0.4,
                y: 1.08,
                orientation: 'h',
            },
        };
	
		var optionsPercentage = {
			displaylogo: false,
			modeBarButtonsToRemove:
				['zoom2d', 
				'pan', 
				'pan2d',
				'resetScale2d',
				'hoverClosestCartesian',
				'hoverCompareCartesian',
				'toggleSpikelines']
			,
			toImageButtonOptions: {
				filename: 'Bismark Alignment Stats Summary',
				width: 1600,  <!-- width: stacksDivPercentage._fullLayout.width, height: stacksDivPercentage._fullLayout.height -->
				height: 400,
				format: 'png'
			}
		};
		
		var optionsNumbers = {
			displaylogo: false,
			modeBarButtonsToRemove:
				['zoom2d', 
				'pan', 
				'pan2d',
				'resetScale2d',
				'hoverClosestCartesian',
				'hoverCompareCartesian',
				'toggleSpikelines']
			,
			toImageButtonOptions: {
				filename: 'Bismark Alignment Stats Numbers',
				width: 1600,
				height: 400,
				format: 'png'
			}
		};
       
		Plotly.newPlot(stacksDivPercentage, stackedArea(traces2), layoutPercentage, optionsPercentage);
		
        Plotly.newPlot(stacksDivNumbers, stackedArea(traces1), layoutNumbers, optionsNumbers);
		     
			
	</script>

    <!-- ### METHYLATION CONTEXT -->

    <script>
        var alignment_plot_colors2 = ['#01665e','#f28f43', '#0d233a', '#492970', '#2f7ed8', '#8bbc21']; 
        var meth_plot_colors = ['#0d233a', '#2f7ed8', '#8bbc21', '#1aadce', '#910000', '#492970'];
        var methylationDiv_CpG = document.getElementById("methylation_context_CpG");

        var methylationDiv_CHG = document.getElementById("methylation_context_CHG");

        var methylationDiv_CHH = document.getElementById("methylation_context_CHH");

        var num_samples = {{num_samples}};
        
        <!-- CpG methylation calls -->
        var traces_m1 = [ 
            <!-- {x: [{{x_values_methylation}}], y: [{{meth_cpg_string}}], -->
            {x: [{{x_values_methylation}}], y: [{{p_CpG_m_replace}}], 
                fill: 'tozeroy',
                name:'Methylated CpG',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                fillcolor: '#0d233a',
                visible: true,
                line:{
                    color: '#0d233a',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },
            <!-- {x: [{{x_values_methylation}}], y: [{{unmeth_cpg_string}}], -->
            {x: [{{x_values_methylation}}], y: [{{p_CpG_u_replace}}], 
                fill: 'tonexty',
                name:'Unmethylated CpG',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                fillcolor: '#2f7ed8',
                line:{
                    color: '#2f7ed8',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },
        ];

        <!-- CHG methylation calls -->
        var traces_m2 = [ 
            <!-- {x: [{{x_values_methylation}}], y: [{{meth_chg_string}}], -->
            {x: [{{x_values_methylation}}], y: [{{p_CHG_m_replace}}], 
                fill: 'tozeroy',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                name:'Methylated CHG',
                fillcolor: '#1aadce',
                visible: true,
                line:{
                    color: '#1aadce',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },
            <!-- {x: [{{x_values_methylation}}], y: [{{unmeth_chg_string}}], -->
            {x: [{{x_values_methylation}}], y: [{{p_CHG_u_replace}}], 
                fill: 'tonexty',
                name:'Unmethylated CHG',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                fillcolor: '#8bbc21',
                line:{
                    color: '#8bbc21',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
            },
        ];

         <!-- CHH methylation calls -->
        var traces_m3 = [ 
            <!-- {x: [{{x_values_methylation}}], y: [{{meth_chh_string}}],  -->
                {x: [{{x_values_methylation}}], y: [{{p_CHH_m_replace}}], 
                fill: 'tozeroy',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                line:{
                    color: '#492970',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
                name:'Methylated CHH',
                fillcolor: '#492970',
                visible: true,
            },
            <!-- {x: [{{x_values_methylation}}], y: [{{unmeth_chh_string}}],  -->
                {x: [{{x_values_methylation}}], y: [{{p_CHH_u_replace}}], 
                fill: 'tonexty',
				text: [{{filenames_replace}}],
				hovertext: [{{filenames_replace}}],
                line:{
                    color: '#910000',
                    width: 1,
                },
                marker:{
                    symbol: 'circle-dot',
                    size: 7,
                    line:{
                        color: 'black',
                        width:1,
                    },
                },
                name:'Unmethylated CHH',
                fillcolor: '#910000',
            },
        ];

        function stackedArea(traces) {
            var i, j;
            for(i=0; i<traces.length; i++) {
                traces[i].text = [];
                traces[i].hoverinfo = 'all';
                for(j=0; j<(traces[i]['y'].length); j++) {
                    traces[i].text.push(traces[i]['y'][j].toFixed(0)+" "+traces[i].name);
                }
            }
            for(i=1; i<traces.length; i++) {
                for(j=0; j<(Math.min(traces[i]['y'].length, traces[i-1]['y'].length)); j++) {
                    traces[i]['y'][j] += traces[i-1]['y'][j];
                }
            }
            return traces;
        }
        
        var layoutReads = {
			hoverlabel:{
				namelength: -1,
			},
            xaxis: {
                title: '<b>Sample [Hover to identify Sample and Numbers]</b>',
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                }
                
            },
            yaxis: {
                title: '<b># Methylation Calls</b>',
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                }
            },
            height: 400,
            legend:{
                x: 0.45,
                y: 1.1,
                orientation: 'h',
            }               
        }
        var layoutCpG = {
			hoverlabel:{
				namelength: -1,
			},
            <!--title: '<b>Sample [Hover to identify Sample and Numbers]</b>',-->
            yaxis: {
                title: '<b>% CpG Calls</b>',
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                }
            },
            margin: {
                l: 50,
                r: 20,
                b: 20,
                t: 0,
                pad: 0,
            },
            legend:{
                x: 0.8,
                y: 0.9,
                orientation: 'h',
            }               
        }

         var layoutCHG = {
            <!--title: '<b>Sample [Hover to identify Sample and Numbers]</b>',-->
            hoverlabel:{
				namelength: -1,
			},
			yaxis: {
                title: '<b>% CHG Calls</b>',
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                }
            },
            margin: {
                l: 50,
                r: 20,
                b: 20,
                t: 0,
                pad: 0,
            },
            legend:{
                x: 0.8,
                y: 0.9,
                orientation: 'h',
            }               
        }

         var layoutCHH = {
			hoverlabel:{
				namelength: -1,
			},
            <!--title: '<b>Sample [Hover to identify Sample and Numbers]</b>',-->
            yaxis: {
                title: '<b>% CHH Calls</b>',
                titlefont: {
                    size: 14,
                    color: '#4d759e',
                }
            },
            margin: {
                l: 50,
                r: 20,
                b: 20,
                t: 0,
                pad: 0,
            },
            legend:{
                x: 0.8,
                y: 0.9,
                orientation: 'h',
            }               
        }
      
		var optionsCpG = {
			displaylogo: false,
			modeBarButtonsToRemove:
				['zoom2d', 
				'pan', 
				'pan2d',
				'resetScale2d',
				'hoverClosestCartesian',
				'hoverCompareCartesian',
				'toggleSpikelines']
			,
			toImageButtonOptions: {
				filename: 'CpG methylation summary',
				width: 1600,  <!-- width: stacksDivPercentage._fullLayout.width, height: stacksDivPercentage._fullLayout.height -->
				height: 200,
				format: 'png'
			}
		};
		var optionsCHG = {
			displaylogo: false,
			modeBarButtonsToRemove:
				['zoom2d', 
				'pan', 
				'pan2d',
				'resetScale2d',
				'hoverClosestCartesian',
				'hoverCompareCartesian',
				'toggleSpikelines']
			,
			toImageButtonOptions: {
				filename: 'CHG methylation summary',
				width: 1600,  <!-- width: stacksDivPercentage._fullLayout.width, height: stacksDivPercentage._fullLayout.height -->
				height: 200,
				format: 'png'
			}
		};
		var optionsCHH = {
			displaylogo: false,
			modeBarButtonsToRemove:
				['zoom2d', 
				'pan', 
				'pan2d',
				'resetScale2d',
				'hoverClosestCartesian',
				'hoverCompareCartesian',
				'toggleSpikelines']
			,
			toImageButtonOptions: {
				filename: 'CHH methylation summary',
				width: 1600,  <!-- width: stacksDivPercentage._fullLayout.width, height: stacksDivPercentage._fullLayout.height -->
				height: 200,
				format: 'png'
			}
		};
	  
        Plotly.newPlot(methylationDiv_CpG, stackedArea(traces_m1), layoutCpG, optionsCpG);
		
		Plotly.newPlot(methylationDiv_CHG, stackedArea(traces_m2), layoutCHG, optionsCHG);
        
		Plotly.newPlot(methylationDiv_CHH, stackedArea(traces_m3), layoutCHH, optionsCHH);
        
            
    </script>
	
	<footer>
		<a style="float:right;" href="https://www.bioinformatics.babraham.ac.uk/">
            {{bioinf_logo_goes_here}}
		</a>

		<p>Analysis produced by <a href="http://www.bioinformatics.babraham.ac.uk/projects/bismark/"><strong>Bismark</strong></a> (version {{bismark_version}}) - a tool to map bisulfite converted sequence reads and determine cytosine methylation states</p>
		
		<p>Report graphs rendered using <a href="https://plot.ly/">plot.ly</a>, design last changed 15 Aug 2018.</p>

	</footer>
</div>
</body>
</html>
