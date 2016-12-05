<!DOCTYPE html>
<html>
<head>
	<meta http-equiv="content-type" content="text/html; charset=UTF-8">
	<title>Bismark Processing Report - {{filename}}</title>
	<style type="text/css">
		body {
			font-family: Arial, sans-serif;
			font-size:14px;
			padding:0 20px 20px;
		}
		.container {
			margin:0 auto;
			max-width:1200px;
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
		.data {
			float:left;
			width:500px;
			max-width:100%;
			margin-right:30px;
			border:1px solid #CCC;
			border-collapse:separate;
			border-spacing: 0;
			border-left:0;
			-webkit-border-radius:4px;
			-moz-border-radius:4px;
			border-radius:4px;
		}
		.data th, .data td {
			border-left:1px solid #CCC;
			border-top:1px solid #CCC;
			padding:5px 7px;
		}
		.data tr:first-child th,
		.data tr:first-child td {
			border-top:0;
		}
		.data tr:last-child th,
		.data tr:last-child td {
			border-bottom: 2px solid #666;
		}
		.plot {
			width:650px;
			max-width:100%;
			float:left;
			margin-bottom:30px;
		}
		
		.fullWidth_plot {
			height: 600px;
		}
		
		.data th {
			text-align:left;
		}
		.data td {
			text-align:right;
		}
		footer {
			color:#999;
		}
		footer a {
			color:#999;
		}
		.error-msg {
			color: #a94442;
    		background-color: #f2dede;
    		border: 1px solid #ebccd1;
		    padding: 15px;
    		margin-bottom: 20px;
    		border-radius: 4px;
		}
    .error-msg h3 { margin: 0; }
    .error-msg pre { margin: 0; }
	</style>
</head>
<body>
<script>
{{jquery_js}}
{{highcharts_js}}
{{bismark_sitrep_js}}
</script>
	
<div class="container">
	<div class="header">
		<img alt="Bismark" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAANYAAADbCAYAAAARfW/wAAAABHNCSVQICAgIfAhkiAAAAAlwSFlzAAALEwAACxMBAJqcGAAAABl0RVh0U29mdHdhcmUAd3d3Lmlua3NjYXBlLm9yZ5vuPBoAACAASURBVHic7Z13mCRV9f4/ZxOb02xiMywsOecsSUDJQSQJSFRAERH4GlBRUBAQRCQnCSIKgiBJUZDsyg8kCUqSKHEBgWXZcH5/vLe2q+6t2Znp6Z60/T5PPzNVXV1d3V3nnvwec3ca6ByY2TBgDWAtYElgBDAy/M3+HwzMBv4HfBj+/yj8/0/gAeBB4Clv/JhdBtb4LToOZrYUsCWwAbA68AHwV+A/wJvAG+HxHhKgT8JjtrvPM7PewERgCSSIS4THcsBU4CHgXuAe4AF3/7CjPlsDRTQEq84wsxWBLwHbICH4M3AucLu7v1fD9xkGbAxsCnwKWAF4FLgV+KW7/7tW79VAy2gIVp1gZp8GjgK2At4Hfgn8wt3/2UHvPwLYGtg1/P1/wKXANe7+v464hkUZDcGqIcysH7AnEqiVgCeAXyCN8UEnXtcg4DNIyDYEzgbOcPePOuuaejoaglUDmNlIZO4dDgwBrgUucfc7O/O6ymBmU4ErkI92HnC1uz/dmdfUE9EQrHbAzJYGjgT2Axxpgp+4+1udeV2tgZmtDOwD7As8Bxzt7vd07lX1HDQEqwqY2VjgHGAH4OPw/ynu/kanXlgVCJ/ldGA74A7gceCs9n4WM1sW+NjdX2j3RXYwglbfDZhRtdXh7o1HKx9ALxQIeAnlkk4Hxnb2ddXos/UD9gDeAt4FjgYGteN830Ba/CXgYmDJzv6Mbbj2lcK1O4rg9m7rOXpVJY2LIMzsQJRv+h1wHTDN3Y9y99c798pqA3f/xN1/hW6qK4DvAC+a2XfNbLEqTpmZlTcBqwFPmtmPzGxwba64rpgZ/r4BHAJcEwJTrUbDFGwBZtYHOAX4Clq9TnL3Vzv3quqPEEncE/gmSmTv4+6PtPEcDyO/8x1gEvBVlPze0ruwHxo++wfA5cC/gB8A17r7rq09R0NjLQShUuJuYCNgLXc/fFEQKgB3/9DdL0Aa7F7gQTP7v1D90Vq8B6wM/M7dz0SlWzOBO4Nv1yXhqlj5BF3/T4A5wC5mtkprz9EQrBKYWR8zOxa4D7gaWNfdH+7ky+oUuPsH7n4oyoNtCzxmZgeb2YBWvPxJYC8UCMDd3w7n+RC4y8zG1+mya4GZqJRsNnAyErSvtPbFDcGKYGZrAn8H1gfWcPcz3X1eJ19Wp8Pd73D3DYAvomqSF83sBDMbspCXXYoKiffMnedj4EpgAvBXM5scvyiYYp2NxVBOEuCHwI3AnmY2qjUvbghWgJkNMrPTUWDiB+6+g7u/1NnX1dXg7g+4+y7AF4D9gafMbOdmjv0bijDGjv99wKqogv/8/BNm1jec8/O1vvbWIiwWw4Gh2S7gMqA/cHDJ8aPjfQ3BAsxsG+AxtEqt6O7XdvIldXm4+y2o0Pcy4GQzu8bMxpQc+hIwN9q3krs/i8q91jOz/H24Barg/3kL2rCeyLToL8Pf9YGHUZRw3/yBZrYM8JKZXZzfv0gLlpmNNrMrgbNQ1Oswd3+/s6+ru8Dd33f3bwLLooXpUTPbIzrsZVLBym7cS5BWWDb33K7Ai0ATOROygzEFXfPtYftIZLpeBUwPCeQMJwN9gf3MbHi2c5EVrGBq/BM1EK7q7vd28iV1W7j7PHf/AbATcJKZ3WBmi4enXyEVrHXD655DCel1AUIwYwcU8AD4lpk11fv6SzAZeDnnW9+NzMC/hu2NAczss8CawPHIXFwnO8EiJ1hmNsrMfoPCqHu5+6HeiZXnPQnufj+wClqsnjSz/ZDGWhD8Cc7/f3Iv+zuwrpkZ0mB3oKbPB4BxhIhiB2M54IXc9vtAH6RJAYaY2XYokb4LcHPYv2gKlpnthFo5PkC+1G2dfEk9DsE83BtV+/8UOIyixpoHfDu3PQNprCOAzYBT0ep/Jvqtptb/qhNsCfwptz0eLQZvh+2dgd8AB7r7gyEVcwBBk8EiIlhmtqyZ3QD8HDjA3ff3GnbvNpDC3a9GyeFXKQrWKGD33PYrKAhyIrIgZiBT7E1UEDypQy44wMwmIY3129zuq9z9GeR7gRaAy6Ig10vA+lnpU48WrBCcOBtFdJ4FlnP3mzr5shYZhHTFAxQFa+vosH7oPjzY3a8J+zZFgjUP2C3QDnQUtgJe92KP2qGh4mRpZNrujDRqHm8BAwjmYJ8OuNBOgZkdBpyEhGotd3+8ky9pUcV8cj4W4v24M9tw97PMbMNQAJzBkGD1D6/vSCqBrZAJmscaKPJ3kbtfGL/AzNZBmheUt+uZgmVmm6PgxMHufkVnX88ijteQoGQYicymPOIGS0caoD/wnLvPr9/lVWBmQ5FGjRPe7wJ9Q9VIGWZQSYIPgB5oCprZAah6Yo+GUHUJzEd+VobJyJzK481oe567z6GisToK+yPauD9G+2cijbUAoUIEgCD4S6MFYT70IMEyswFmdg2KQm3t7jd09jU1AMiMeg3AzKYD67j7AkEKYfadotcsE/6OQIniuiNcx+HAaSX7NyEnWKGV6JDc9iDUJGqEMqgeIVihEe96FEZfK+RTGugamEJFYy1PUXvhagi8LnrNq+FmXQ21rGBmTWZ2XUjK1gOfBaZRyVVlaEKfIa+xBiLS1QxbAseE194JPUCwwg9wLfA0CqUv8pXoXQzzKApTwQwMdYKxj/VfdOP2Bc4Ox1yKNNtQagwzG4g6pv/l7k/mnwsNma+Qk5VQ9pZfvDcEegOXZv5gtxaswDL7d+Af7v4Vb7RDdymEqu+lqfhQA0kDFwcBK+Ze0wvluTZF/Vz3A9fkjqlHlcwVyJ9rrk50MdSPlcfE3P8bhb+3ZDu6rWCZ2dbA34DL3f1bnX09DZRiG+CDXFTvY1LBAkXVMkxFCeMtUf7rBTQYYtvwfE0Fy8xWR/WJB6I2/Ph5Q5HMOdFTfwnPD0Em67vkPke3DLeb2brI/DvF3U+q4vWZQ/q2uz9W6+trYAF60XJEsMnd38ltj0T35Rph+xPgAhR+B3Uf1wTBNz8P5admmFn/ksP6o88RC9awcB9diEzWw/JuSLcSrPDBv49Wl7Pc/XtVnGMEIglZBq2Iy9XyGhsoYAuKgjUAeCY6Zh0z65XTasPQTXwJ6vUaC7yOIoRQrvGqxWnIpMuqQcrIcuYiHys2BbdB99DngDPd/arQj7Yn8Gqnc7i15YFq/Tx8kGpevzrqsVkP2c03dPZn6qkP1E7hwFG5fWcBq+S2Fwd+E71ua2D73PYByL/aFZUa1er6dkTckGvn9l3czD3j5LgFUVj9aZSzehdVa/wIaVMHXu42PlYo0z8MON/dv1rF6/cGvgXs6grHH0QI5TZQF0wFZrr76bAgKLEnRVNwFXQjxsi3lTQhrbcm5RqlzQiFthcC+7voAzJMKTl8KMXeLFBj5nQkYMMQH8aKqGLjAuCDbiFYoWnuYmQaHFrF6/dFIdHPufv7wbY+BFFD54/rZ2YN07A2OAQV4AILqhP6uJiaMrxIUYgy5I/5j8sHW4MaCJaZLYmid39z919HT8d+FEhw/hDta0JTNLcGRrh7P+BQVxtSE/BGlxes4CBejro3D/Cgi5s5ds2SffsAk1wNjdmq80XgSg9zosysVzjuVuACM7uzGf6GBloB07CILYDjcvv6oBldeawKlA3EywczBgZt127BMvHJ/w0N/5tiOXbbUEG/QcnL+gPPR/vuA+5z99vc/d3w2TL2ppWB17u8YCGTrRewp7ec/D0s+8fM+ppooc9D9m+2vxewN6HsP9wEf0Zq/XDgy8DayP5uoDqsgwYiPJrbtye6qfNYlTTE3cuLc7s+QRHcwai7uCoEIboK+dVfQaZqvh3lfTR1JUYvioIOanxcMrc9CfggFCtMA57t0oIVyDlOBL7hIk5sCfnIzV6ohGaXSCA3Ap5291lmtivwK+BL7v5LgHAzPOSNdv2qEH6zb1Jpo8jwMBrdmsdqpBprhWj7j+i3vMmrnIBiZpugZPNbwCHBCnrQczWLSMhWDNonj2VJc2dvUlx430PRww3QAn1llxYsFFrvA/wjv9PMRubISvK4PvvH3S9196NQ5CmPocBYM7sJccRt6WF8qbs/GcLxZbZ2A63DOSiFEYfFjya9QZd291gAl4q25yNeiYuquRjTHLDr0W+6m7vPRWH/2KycDdwans9jfMl19ybXsxV8wDdQxcjf3f2xLitYZrYCClRsV/JhP3L310peVojymVhWy6ZEfIhyGNu4+8xw7Ljw3OpEq60J65rZRjTQLAJRzC7ALFJNtBK5QIWZTSA0BUZ4PXfMGGQqfoT837Zez+rIzB8OHOIVOob+8Xu7+yzKF9T7iQQrmKqn5t7nWFQpsg8KsnXpkqYzUY3YAyXPTY93mIZpLx3t3gOZAHk8jRog/xKZiBnl1sbkVtvggz2Kcmg/DDdEA+U4CCWATyfVCAMpmoJLkBbkGqHFJKAvSgz/ohX+dQGhq/cOdB9dGL3XAKJ7P/hgCaMtivLNLNm/fu7/ScileAMF2rqmYJnZtugGPx6FyWN8umTfeojXIo9NEXdgHrPdvWylzBzUDQkrq5mtj1bKw919TeR8f781n2FRg2kyy3cQA9Ms5FNlz/VBfm2+aXECRSECmVIfhNf0Q8WxfwJ+3MZr2QT5Zme5+A77UwxAjKDCy55hcST8MUaRtpJA0TpaPbz2a5lv3uUEK5B2nIwSwc+jJGKMsikV/UoEZnjkoIIYduL3HIxWHJDz+bKZfQGp9Z3c/a7w3PsU28wbYEE37WWoCPV6FFXNt3csCzwUvaxMsD4NPBPugUtRwnb31morM+ttZscjYfyJux8fnmqiWGM4kVSw3qJCyJnHCEo0lrt/Et7zOLSoH+/uv8me73KChXJMU9GEB6jwZ+cxq2RfrNr7EBVshhtgtZLXfpcKk9B/w3vvCGwahYw3BO6igRinorq5PZFjvxnFGsH1SBPBE0kFqxe6iW9D3/+OXizQbRYm2ue7kNY8OGiqDKMoMkVNJHBT5NAbRfdi9C/LnZrZFmZ2LYpaHxa9X9cSrJAHOAHVAv437C6bOlHQQkFgYvt4CqkDvS7lDvP0XIDkQHdfDSU3F7ADhQLgtUi7XRdpmLjajwD2DRG+NYHXvNg1UCZYE4i6idHNfwuwObrR/0sLCJQMhyOfbh7i+7skOmx4JByjSXk2DqUZSyh6v2mBAuKPKGCxl7v/In5RlxIsFJLtjxiWMoEZWXJc/IOsSLr6LUUauFiW6AcOzXgLzIJccnJZihpvbeCvmQ1tZpubOMovb+Ez9ViY2UqoNu4X7p6V/YwlrVRoTrBeM7OhZvZ5M/s1WkSnhuf7oS6G5t57tJl9D/k/BwObuPsmlGud2Ox7l/QeGtTMa0eEqPCqZvYXFJzZGY0fWtpFTJqgy7SNmNkSaNL6UVkIHEX5XoqOm0zEmIMq1ePARZlgLUNa9zWGtCIA4Gh3/31ue03gqXANJyCNeClwvpmt5WJw7bEws8XySfqQR/wDuiFPzR06AjU0ZseNQL/FS2F7CDK9VwPOQKVKzQ3OPtTMTs58LDObgqyOzVH1zADUQb6Zu/8v+GYFsy0szrHAvF2yz1CpUoxVUDXFWVQCaX8N5zjAzD5GhQlX5c3WLiFYIcx6EfBbd88PIovJ6UGRvjgD34tywbo+2jedoA1z6Its+vz19EPCmsckNMvpceAud983HLs+6m7tsYIVHPRVUPoiC/b8AeWXjqVYhTACmXEZlkamYWZqr4V43QciTbYwTALOC/mxddCQhLeROZ5pt0lZzScKmMSNkE2kpUqLl+zrRbnv3jscuyMijFkPLcZboaTyJCRHhYLuLiFYyL5dHzHl5JEVTeaRNZ7lsQZFrm2A0SVZ/SZ3fz3adwwKmOSxPGkU6xz0hT6GopYZbket3T0SZrY7qrXMEunTgF8jrbQl6lPKL3QjKHIBOjkz0N3/HELz/0IafzSaS9zckLkDwt+3UFT2YHe/LlzLV8gllJFgxVUSZYI1hdTHWkBdtmCH6kr/FdIEb6NFJH7+CeCWOHLZ6T5WiOaciEpB4hVjOqkQrUpqH29UUolR6PgMUcKyFWkxTxlOd0JtAXlMA77s7j+OHOEPKBZz9hiYJl1egLRxrxCoeBiZxJujRe7k6GUjKJrb80j9q8EoWXwUCjqUzRyei3JF30H+7VgkXE/ljhlGUbCGUK6x1oj2vUPK9pT1VuUxiMhfNI3UzVqL9kL36MXR6zpXYwUT8ELEsT6u5JDFPaUXXtpFSZVHIbEXVpJYWJYkqqQO9n9cLgVqu94/2re6azxo9trRKNDyCeUC220QTN/PAc+4+wNh36fQqJr9kfm9FrqBvuLuF4RjRpF+zyMo+i8FjRUwBi1IKyBtOCu85hxklv8XdRGfFV3jBIqt/bFglWmsONQOci+GR/v6kCaIB5F+vpWBwWY2C/gBypclcwE6W2OdDvzP3U8l/aBQ6XHJIxYqSBN4E0nV/3TgXVOTY35frP1AQh63M8RJyr3QKjqY8hB+t0AQjrtQKc54U2/a15DW+VZIer6Jom/bZEIVMJvUVB8RbQ8jFazRSFB/j6JrRyL/+gR3fxD5bHGAaknE4563RIaTClaZxoopo4eR+t/9kZmfR5lgPYEE9UFkmh5PCTpNY5nZQcD2qBwEIjUcHOT50b5+aM7tSejHyR7LmFlWmzYcCcZiZhYHKgC+YWZPoYr5viiqlH+PLYH3XNzh2b4BpJxzW6Lh1FtRXkvWpRGshX3RjTE17O6Hik7XRivxmQDu/t1wfNz2sRKprzKYYjR2LKlgrU7FZDwU+ApF031xigEQkCke+71lGiuO9o2iOEQO5EPHPvkA0s83mFSwLkSBjKvRwhOTzACdJFgmDu8TgM+4+3shTBqvUBPI3bBmthWqSM/6df6LhONetOq9i0yK+DEbfUEj0Y85Ivf/EaQ5l9lo4FkeaxEJIHLaPzF1nnYrwTLN9b0M3eCHoeDBgcCVyBRbj3TlH0eqmTdG1eN5xA2OYwltP+F3PgX5VvehZLybmGjfjl4TW1PvkutADj7zKhQFewipT95EWiiwHGmIfzZpJLigsczsO2h065/d/QssBB0uWOFG/D3wXdeISdAKFZtkE4CZQVvcjnIId6LI4YySGsBqruV14Odmdm0uD3Uk6cq4ProRs9eNpCKQQyneFF0KQdNsBDzi4vtYH622byCe+1dCOH0+CkR8G/H/xX1sA0v83cm0TKA5NlzGN1BbxUph/0G5vNggim0hZay0G5Lzr9x9rpndE0XjmtNY8YIwpuT8r5FqyUHo4g9A98WKwE0oXbBQdKiPFYIKvwImR/mqCaSNcRPQ6vNzZJrs5u6buvvNtRCqgAuQ+s9XrK9AqrHWpbgybkpFgw2jhiSSdcCFBKJ+MzsG+VMzUCT1lZBAfRAY5u7fDMLzVdJcYbyag8LW8VC4xc1sPzPb38xOQfWDTyBNtRLSDHMpmoeDKN78fUmT+y+SG54dEF9jmWANyJv1AXM9HZX7IrnWEjNbFeXtvom+w7nA5u6+nbvHpKMJOkxjhZXzHMq7NydSLljjkerd1t1vpsZw9/kh+nW1me3k7r9DkaE4A79M1Gy5D6oeAJmZcUFnl4CZfQnl6GYivofPosXqu7nUxorAs16kIliF9KYtSylMIaexQmBoCbRgNXdv/RTYwd3zi9FwpNmyBe0Bd48tmH+T8pDEI36Gkmqi2MUAGGpmd4XzZR3FI4BtTf12SyKN/R/gXOQL3lKisZtFR2qsM1B36aGkq8oEUurhcSgEfFU9hCpDyNo7cImZjQfu9eL8pqVJO0vHUWnNzgoyuxRMPW2noe91BLpZ1kfh4rzPsSapKTssV82QCUwcXDK0IOY11gjkc01F5maMN6iMr81jBEUf9p0QvMpjPGpSzd5/KOmCNphUg8bnAQnga4i+bFu0mL6BtPKDqLRuGXef6u5fdvc/tEWooIMEy8y+jIRkE9fQ5DJ/KrbVs/D7d6k/folunNNJQ64rokqDPF7KNJi7/6OZxslOgZkNNLPfATcgM2wG8ptWdRGVnuNFYp7lKc/l5TGeNN2wONIGseZ5Czn895Lm9y5EN3V8riGRabYzigLmsRxFbfQpSqonSgSgrKLjXXf/PEpy/wtpvu+5+47u/nV3v9LdkwEJbUHdTUEzOxqFUzdx93+FkHksRGUELsNQ3iKuAawHbkDm0u6kJtB05LACC9pHWrSxOwMmhqQ/oCqCjd39XjMb5O5XhufLyP2XIldlErRTLBDjCcJnIrw8lkooepqZzUOm3K5IA+xNuQn2TRTij4lhYg7HKYRpHjm8Q9EP25I0AhgX4PZGgZAYL8OC8qrV0MJ6v5n9DVkgl3mRWLTNqKvGMrPTUNHrgbkVYCSpsz+ElHR+GGlZUV0QVvCrwuYF0dP5+U4g/yOeqt7pCNXmd6GbLROqXhTD4Z8mXRSWoriYjEPtHCPMbDdTa8h4Kr/FTFSNsScyxf6FCqDvQ2H00ZQL1UykscYCx5vZw2Z2fDh/nCtajbQQYLgX+QbXpZjDgpSuegSRdWRRA2w454+Rhl2HZhK+bUXdBMvMNkMhysfd/fbcU3GbNEiw4pV0KOUcBPVCVu+1erR/MkXBqgnVca1gwkFI2JcGLskFWnpFNZRvkEslBMF7J6zce5rZDJTHGoMipdegqOlFwEgz255yHpGWcAvyWQ5Cua//oJrP74fzTzGzU81sk3DjP1MSyYt9pTuRtsz368WCNYqcXxZQGBhuYuf6Ggrn340if+1Pn3h9Jk0MQiVFxwEnRc9tDHw12ncXMCXa9280VK4jJ2Tci5J/+X1XRNvno3btDruuhVzvcmjM6Ksot/ITYFTu+eOi408BlshtLwmcGP4fj4IK3obHzFYccxXQN7qOEchvLTv+HST8O0SfZbXoHLuG4+ehxPF5KGDSL3fMBohTPdvuhRbKp5D590R4/eOIZq9mv029fKwTUfj8FFI+uDKNNZRyHysuRak3TgWuM7Pl3P2fpqa8OGs/2NNq+A6HicX3UhScWNrdPzSzG7xYoLwtgeHIzDYEtgOWMrO5SCutDfzPNB3zY9qeNvgJmuDSnGXxY+CbHu7qDK5G1t2Dv7o9+u3PRRUaA1BgItOS/0Rm5hJm9kMqTYr/Q0n7fZF5ORSZcrPNbA7y4+fpo9sPwzVmn282qty5Bvly93gbo34toeaCFTL7X0BRqPmRqobmfax8bd6qlGfM6wp3/52ZnYs07b7ItMqXVfWnC7DkmiZaXo1uuG2DUA0GZplG1AxCAYxXrDLUbSnU37ZsdLoBpMGD1uI/SFvEFHVzkaZoib32GCRYR3qRN+LnIZw/KTwykppfICEagXJQr6Pv4F53PwbAzMYic34oMvv6o/v8DeR/vgq84qmpWVPUVLBCHuhSxMP3Ysjqx1G9JtIwaZmPZbRDsMIPMw2FkycjE+NOTxOPMY4AHjazPZEt/kLuuZXpGoGLGcDvUE3fiSGa1Q/lgm6jOKVyWzN7lOoXhGfQ4rJWyXPzSaNu76EZZC1aG+7+tJk9AexrZle4ptFnzzmqhniRlOG4V3MaxtXIekvZcx2JmglWKKz9E1KrWYRtIinTTmuCF5n5tSYt0I2FOrf8KtwLaZqVSBvoZoYKi2bP6apBOwvV151Osf9nDSIe+c6Au88zsywPswfwPbSIvEil1ebPKILZhCJoMf5EZZFoQpRlZSxFo0inbWR4k2JN4X9QYXVcjrQwXIeaGS9A6Y4WUWuzrS6ooSN9PXImN8zt+xTwo+i4CymOp8w6e3tFx71CyejKkve9m7Y53K8DQ1tx3r4l+y4GxtbSya3h9z8ICdDBiOQSZOYdhJzz/HcwD9Vr5l/fB0Xo2vJdrpD7/4FqvhsUHcwCIb3b+z10lUdNwu2mgV7bI57s/Ao2mTTZ2ESxLnAw8G9PV6HXaZ0z3ZrP8B5qyJuJ/IljWnqBRzZ4MC3X95Qzo0vA3T909wfc/XwPkwrdfZa7X+DuK6KkaxYuX9bdY9rk71GpPG8N3qNCTXcJKgBo83fj7o+gCPJwFEzpEahVHuvHyO7/O8XepLGkhZODKJqHQ0lzDSDTsDVcEmWjNjO8jshIRrv7OqhdBVQZUA26XE1ga+HuL7r7je5+p7sXIp1mtgPwf2085R1o0VzL3b/orZtf1hyOCH9bbMfoLmi3YJnZYSjncBpaFfPh3kGkgvWhBxsgYAhFgpAMc4F1gqZYGJoLJvwD/egX57TP/eHvFNOIl1bDhSNaPrLrwswODXWb+X0HAddSuRdubMWpnkQNkj+nnNqgrbgPmf57Buun26NdgmUiYzkNRc6uRYKVF6TBpKZgXH08hLQ0BSRYI2nZPClzlH8PbODucStKXgh3buG8PQZm1t/MLgZ+RrEu8AiU8M4a/P6NFskyXpEMr6N82LkouNRu09hVxPx55GtdHqLL3Rrt1VjroVL7M1ydnFMotiAMpkRjRdtDSGvFoBLRa2mKfWxGnocmhJQ1H+Y/b8xh2CNhZhugAM9kYF13fyjsX46UvPS1YE3cRDlmIWq4Y5EAvu4lk0DMbDEzO9DMbgk1gevHx8Rw93tQg+UawEOteU1XRnsF6+co15QlAidR9LHKTMG4E7U5wcqOW6aFa3iWSo3YjWiecHPh2Hxx6BItnLfbwswmm9m3zezfiDTlJnffwt3/X3jeUHg7/i0yrr1DSPupXkWDz09GkUdIJ1/2NrMjkQWzG6Ji3tvdy6ibE7iSxDuiBfkvwUztnmhHeHd5dEOfktv3cnTM74Atctv9gUujY/ZDbffx+S9Fptv/gEELuY7+VEK+41q45s9RDBcP7OywbC0fKGR+MkrcOgowDI2OyWaBlYXP56K2k/3QpPqN0I2+NRKq16Ljj82dd2UUwHoccam353OsgEx8Ry09S3b2d9vmz9COD/+N8MFXCtsjUNg8f8ztKESdbY8sEawjKCmARDmjQ8OPPWAh1zElXMeLrbjm06IbY1pn/wA1+RGVxP0/ph5nnwAAIABJREFUVHXuSIsfT64gNRy3MjKdW5un2g1VdDxR8tzMTGhRn9UnYTFM8n9VfqZBqBbQkQl6QndaCNtjCm6EeK2zjtsm0hbvwRS7PgeQlvYPQUWRCxCaISeiCuT3WTg3ehaEiKmOyxDb7TETUbdDaHu4G7W8f4SCNtNc5JfZ1MEJZvZ9lMRN5jcHPAmcjeok90cL5m/COZ4vOf5sVMD7M1R0faK77+c1qsFz5eX2RemS+ag6I6ag67JoT0nT6miFyjCKVLBi9p2BpP5UGSniUKSlPjKzq5BpknAomNkaiKL4y+5+Tvx8dOxipL1WMd1Vt4KJSu7PqKTrYhQCX8rMlkfVFWsSyHjQZ52NSogeRQne99HvM8PTCGoecaL+dhT4uBCR1ezn7pclr6oB3P3iUAt5G92Iv7EqwQoVxBNQL1CGMsEaQFGwBpASyfQlFay+VDpfbwVuMLMJnk4PmQjs4WJXagmrk5I0xow+3QahSfFKFDX9lrufFPZ/HS1EGeajessrEY1z2XC1lpBfkP6IIqqnIaG6pl5ClcHdHzezD6H7zH+uVmNlplleIEaRUpj19SKt1kBSwcpW0jzepDKh8S/Iz9qHaHq6u9/QhmsuC99Wc5N1FWyNbvDnKYbNf4S+z3dRG/713goevOZgZitTIfZ5D5lmOyMekzm0vWKjmmsYgIqM68bWVWtUK1j7hr95gWgiTdbGPBYDSPNYfUg11khCYtnVa/Q0ac9Pq2FmE1H7dYxuq7GomGc35v0aF7fIoTV8n6yu8h4UZh+ITEBQGD8ePlEP7IJ+qxM64L1qgjYLlmloWLb65wVnFCmPd6yJBpLmj3qTCtanKA5EeJHyMT8tIvghtyDTNY836EY2ewky/7Bu9NZmtgyqiPgDslIWQ8XMGaVYXMhbLxwFXOe5UaRdHdVEBXeh0juV/6BzSdvYY4EZgEK+efQhFcBe0b6XqCKCF6KLv0PcgDFO9ZKqgW6E7Lsuo35uNcxsieCvxftXRD1bD6M2lHmoqmWF3GF176Y2s10Qa9N19X6vWqIawdoIVTjMplhZ/jo5My+0scc+zEBStp0yjQVFbfgxMKYVBbkLEG6Wy1GbRIy3UJt3t4VroMTjwM5t+V5KsDbwppn9zMzWN7PdTWOS7kG/71bBHD+ESldANkDijna8b4sIvtVpqEigo/lP2oU2CVbWk4RKWZ7xYunQTA+ZvYA4IpjteybaVyZYAylqrDWRoLUlPH4eqrQow5leXkvY3XA+CrV/tdoTuHq3foAifLegruldUePqFu7+TkhVnBJe8igV7RGPG601jkMFADd7+9pSOhxt1VjLowqLy0nbteOK5IGkgjWQcsGKv7TRBI0VeObWAO7w4mCCZmFmp6J5T2WYR8nM2O4I1yjRHYHTzOx5MzuxyvOc4e6D3X2Yu09w9+kh2ZsteNtQEaIzqPz2q7TrAwBmtpKZXWlmr5jZo6YZVJjZEihw8jaq7OhWaGvwYiXkS/2XlO10SrQ9nDQ4UCZY8yNNB+ryzdpLVkaa7g8sBEGbboAiYnst5NBbvWVCmW4Dd7/BzAahAuh4wHmrsTCCFirzmN9APIHZsOzYX27rex6GKOcyQprxKLr8A7Qw9kc0ZieEJPGDaM5Xl9debdVYU1EG/ANSwZoabZdNABxA6xrjxlARyowIpaUcxgmotGdhQgXduAu4Obj7x+7+7yhn2CqY2eamkTalIXoz2w619AP8wt1nu6rVbwY2DsUCbX3PwWZ2A+qO6I/886MRE9S00ISaTUzsh7Tymagk6w0zO8fM1mzr+3Yk2ipYU9CqYaQTKmJTcCzlpmDs25Rdw2gqgrUe8NjCSm6C2dDaRGVr2v17PMzs02Z2L/KXrvAir192zAQ002w+CiKcmXv6PGQe/qiN79sXuRLbI6vkO6gY+jTU9X0cuscmonvsDEQ488fwmtuRBp0Rer02b8v7dxSq0VgfI20VT+WI0ZzGarVgBfNuC1owA1HPVmsDG59v5XE9FmZ2PLI8nkY3dTwIAjNbAVEZTAAOcfejgTlmtpaZ7Y2iwwD7mVmLJDAmfB31au2IWGinu/sPgfkmHse/obFNmYvSB1Xp/xuNTrrR3XdDqZffIoH7k5n9yjQUosugGo01G33guGU+9pPKBKu1GmtAyJusHM7TkhkY1wAuDMuZ2R5tOL5Hwcy+hYYRXO4igUna8M1sI2RWT0Ih/cFm9kcUtLgWmdsZN4UBt5rZMSE8XvaeQ1C1xqnIsnkWaaKdzexy5LNfiQSlOXwxCNBwF0X1a0hznYu6mh/tUuZhG3tkPkTdpVNQDVr+uYej7V+S68UK+64hakYEri55n6fC32OQubBQvjkUqZxH28j8J9WjD6crP5C5nH0HI5o5Zk9UTpYd9x4Spv8Seu/CcZuF/b9EJpujNMwRqIZxJ2QdXIh88tb+Ni09XkJEpb8kDEpASesXUNnTJp39Pbu3odERmWeOKqenAV+Jnv9HtH07sHy076b4BwV+FW0PA+4K/98RC+xCrq+56RXNPfbv7C+/Q39oCUL22b9f8vzU8Ptkx3wAfJ3QuAicFR2/LXB6+H8a0kAf1VCAFvaYj7TntNz1jEWLwCyU1+tU8s+2mIJZOH02IiaJS2liH6fMFOxDWpgbh3jHAC+EyokNKW+yK8MXaRv984yWD+kZCAGDn4fNvyJyzgXPmWi6n0CBojNQQOFEdz/NKwW+k6PTDiLUfbr7s+5+JPqtfo6G0dUThqyUzM/DRRb6JNJkZ6DgRjxutcPQFsHKSGFmo9q7uDQpPtco0jxWWTI49s1Go1Kaoch3apVguSoptqfloAqIBqwrDDfoKHwN9W19iDS1w4LRPg8jE/FgYIK7fw0Fp+K6z4nR9qCSYxZz9yPcfRmkAQ+mvoXOF5rZXgAhl/eBux+CiD+XRcK1Xh3fv1m0RbBeCH8HoJtygWCF6F18rjnuPit3TG9gBU+rJ0o1FpUeoNZqLFy0yTuRCm8eJ7v7V7Obq6cjhMyz8Z/HuvtzZtZkZheh+rtrgNtcA62zSouVSGnlRkXbU8lppvD75ttX/oNC+XOQD/SdknO2F72By4JwjUHJZdz9XBSgGQH82jSbuUPRasFyzWp9A93w71LUWGXnKfQ6uaJ8j5adOtoegxodsy+jTTwHruTlYc08/V13P64t5+sB+CrSLn8GfhHC2k+hm30NVIwct3+MR9FAAMysrCZwbYomX9l4pu2At939SXf/obsvizRkLdEbkc4cGZ37dUTnMAnl4joUbQ23P48GmK1GcUROWQ6prK6vbIJ5mcYaSmUMZzVjcy4lNQm/4e7dplGuFjCzgag06H3kg+4FXIHSFxu6+xOo2uGR3GsWA16INPoY0nKp1SmagmNIWXFXIP0dZqE0ytmkLMnVYhYquM7XLr6Cqjl+BXw+5N46DG0VrBdRCPUBioJTdp6yyellghVrrEkocvgfFOVZvo3XmGnHjHzG0SC8U9t6nh6AvZE5dCRKol+MHPv9ciZ5QbBQkCKeaTamZN97XhwcPppUsD7K7wuab5a7P+buh6M5Zr9s64cqwYtIOz6Y2zcZRTb3R1bP2WYW17PWDW0VrDfQF7g6xURvmcYqS9q2RmM9R6WN5FFSZqXW4tsoBL+Pu59d5Tm6O45AhJePooVmhrsfFWmjlSgOpZhC2sZTprFiE71MY31EcSLmRMTDAYCLHOhkFKyKOf3bguURN0qeGmIK8JGrYHdXpAguL2vqrAfa+iYvoy+9P0XBaq3GKut2jTXW8ijrD2IX+lTbLjGc1P1/7v55d7+ymtd3Z4QQ+iUocHACMv1+SM5vyqF/FFCaQhouLwiWaa50vEiWCVY/RCKaoaxOcxjq8p5C+7TXYIosThOyVEGwfr6PwvPHtuM9Wo22CtadyLfamSIne+E8oYfKon39Kfe7Yo01x92z6NHPgM8Eno0GWoEcx8cyyDy6DHUDn0klIJQd+xnSLu8ppKRAYygGOJYi4m1HlkxsLq5BUbDWI/WZh6EAx0wXQWc2d6u1eIfK4vz16HryyBTB9wPzVF3RVsGagdo+tmbhpmCZGTgU2bwxYsFa0FbiqmM7D5l1DbSAwFNxD9JUW6EAQX/kZyxOmpwfQ5rEn0xRGEA36bW57SbSNEiZxlqTYoDjU6SJ+WEUm2ZnoZrBVg1SQCbkakgbTzKzpcP+kdFxvVGF/tuISq+uaJNghaDAdujD5HNF8XmaE6yydvjYFIyF9EFgHxPrbbthZiPNbPuWj+xeMLNz0Wp/PvAZVGf5GTTB/n0qznweg5BvkkdfT0k93d3zPtAoUq1SECyrkLrmrY25pDnGYRSZpsa7KNw2Rv5XS/nGKSjaORRY0SvTKmOzszfSlmcDuwWiobqhzY6caxTMJcBBoVQGUmEYQophlAtWjHilmY5W4XNr5HheRIUXsUfAzJrQDTPO1a6/M9LyP3X3zPQqi/Yti9pH8ij7jWJN10T6m8dRwQmo2/dvuX19S841jGJ1xjDQIh5yjr+nHEehCSifhP8nocBXZg7H6IOso3PQIrB/yTE1Q7U36s2orClzBOPzlCUUW9RYoacmjkgthVakqVTmMlWF0A27I92IUbU1cPe3QwjbQxXMj1EVeD5vN5k0ObscuZU9LFytFayY7HRAvtIGCVrMwNtEqrGGUCw2iDvMDZm1mwB7UOGu3MLF178yuj+e8DAEAiW447lsvRANxNvI79yPOqJawbodJd6+Y2YZ4X6eo69MsKZR7mPlsSxpQtFcFMmHA98LNWHV4mAqM6B6KrZGi9HXvMhEVSZYy1A0tcaTCgOkwjCdyjznDGWJ/niRfJlUSIcSyqVCgCsue+qPOif+6u5Xu/vmSIsdBuDuT7uYpvLvtTjpAjGPyj36U2BtM6ubr1WVYIU8yH7IRLsOrST5hrky7TSqZF+M6ZRn6glf3pNI7bcZwVzaBtEixyZRT8LhqO3m2mj/FM/RG4Rk7TNeJNZZmigoEXylfM2nob6sfJ1gXxbCtJXDmJJ9Q6kI/AjSaGP/knO7u7+Qe/+RFO+tJlKNNY8g/OHa/4LqF+uCqn2WoHZ3Qjf7KRQ//FCUm4hRJlj5lW4waWQpb14cA3zdzOKC0NZga6RZz6/itd0CISK2DeX5qrjtY1mKkT5QDvGFaN82FG/2lUjvm9GkdYKj8+cKxcCbkArJICqVHyNKrnsxUmGM86Ex38r9pII1l+K9djN1nI/WrmBAiDZtAzwGbJlreR9Gec6qYAqGVu684PQh1VgLviB3/ztytr9VxeXuhcyeu1s6sBvjEOSTlFUxxGxKy5FS0Q0nDaOvT9E8XIGUWnoMKc/kaIqMWO9SmcmVx8e5nq8ywSpoLDMbQ5p7iwUk1mBQNAVBub7BZrYwOoCq0e4oW6gXWx91n15uZpeS8mFkiD/sYIrC1oeW+3fuBr7UliY2M9sZLQAveRUUYd0IWRFq4eYNnBNPRccuR1rV/r6LTyKP0RTHMzUhlqX4mPh1Yygurh8DN3jKCZjXRmWC9ZoXOfbXJS2vWpyir7gq6b02meL3kmnhdgXEmkN7JjougLt/YGY7oBDvsYg05gYzG+nFCRFlgpVfXRcjDXzE15hxE/7LzJ5Eav9+VG/2IdJws4AlgXXCY5Pw2m7N174whIjeCsjEibXCZNJ81XKk88x2A86K9o2gGKkbRRpgyPNAZsgmk2SYSmpmQlH7lQnWjdH2ZqR+2OKIjCbDgxSHN4DC69fntrNIZNXjoRaGmggWgItF9QQzuxDVZe0PvGhm5yE+cEijgrHGGk3aUBfXHA5AmfN7UMh/ReCgFi5vHmo1b82c4u6KrZG5dy7pDTqZVDuNi5K+kPq3oFrCvFYpmzVdJljjotctQ2QuhsUgv5AOJucHhdK4wykuiP+jXGPlhxCWVfk8SbHKJPPTplIH1LzS191fdfeDkHN8BUriPYcELX6/WLDGkoZ7ywTrcVKHtjk8BKzt7j29LOowtNjMolxjxcJQCI+HesyJ0b5+pL9ZmWCVmYJTouqGZUgtliaKi3t8P4wiTUT3oVyw8tin5L0GR9f4Agq4DAlRxZqibiX07v6Mux+KVoQfhve6yszuMbNvm9l+qHYsH73pR1oAGmvVjM+7pTGnH6KizHVCtUiPhZktiTTWtWi1jjXRZHLaImiKmE9wPKlwTC3ZVzZruqCxQoPlw5HGmk4aqYsDKoOiax9FGm38NGkSOS4saCIVrInkFpyQMsoEdCo1Rs1MweYQ2HN+APzAzDZAnNw7oQ8zEphrZv9EmmoZwuQMFAkaBqxmZgciYZqOvtg/ElVql2Brd7+nhWN6Cg5E0cDrEHNw7DuNo6jFBpUc8wmpJphGKkRlLfixKTiOdOEbRSpYY6LtwdF1lgnWyiXX2Z/irLYRpIL1TAnPSRa9ngrUdPGtu2Dl4e73Avdm22Y2GBVRTkUrypjw2DD8HYdMytjBbKmr+L5FSKggdM+6+8sh8RtrrDHRvsGkgvUZ0sqMaaSarbens8ViU3AcKYPTENJK+rEUI4f5nBZIsOL0y7vuHp/nxUhoygSrrJjXkZsyteS5dqFDBStGCH0/QTNUZKG2r7kizIXh9JYP6Rkws0kogPONsGsYqY81Nto3gdScmkBaQzmUnMZaSGK+TGM9VHKuOIAyhmI7/Ue5XjyINFaoJS0IVVhI4omWw0kFq9CYGVilPqROgtUhbcrtwJFVvOZ5yqs+eio+E/7eFP6WaazRFAVrVdJK9yaK2oJwnrzG2pw01J2dP3/Dlw1iLzM/Z1PUbHFRQayxNqPcvxoY7StorFByFXdcDArnfopFSbBCl+dmVbz0am9+gFpPxDbAq+6eJYDjyBrIV8/ni1Yl9VPezvEKZliSoo+1DFHUNhTOErX3j6OYwwLoE10DpGZePGw9Fqwyrbc4GrKQxwiKof0mUq22fjj30yxKgkX1c3VvaPmQnoEQzt6cSisFQK+8vxFMpTgAsAqpxiobEL4JRY01hFTrlOWwxpKGwKeGCpA8ViMVpjxGUdRQA0revyBYISL5mrvnu6CbSs49BeXtFh2NZWaj0dSLtuI10pWyJ2MjpKHyghULSMxXAQqtLxCYULFeNk84jgoOJs0zlgnWMHI+VtBqs0oS0qtl1xEEIm4JGhW9X3OC9VxuewTiZsmjiVQ7vkBFY9U8l9UlBQuN7SyjSmsJfygJqfZkbBP+3pHbF/+mY0lvxncic7mJyC8Lzv0QWtZYhYhgoHMeGpmVTTTDEEXlhn+atMa0iaJPVyZY2bytDBlTc3yeOD/6GeCNwKvyNjXWWl1OsEL18tdbPLAccV6kp2Mb4FkXZ32GmMthLDmNFRpFY19nHKKay2MY0n6xxiozBfN0aatTXq3xWMn1L4ZmCo9E6ZbkfoyKdgdSUppFqrFiH3MEaeR5EhWhfp6eLlhopm21c4LLat16JMxsOZTPuyN6KhasCRRvxq1JV/RxpNG24ailI5/UbU6wrsltl/GdNJEGS0Dh9TlUNG+MMn6M+P0HRNc4gjQqWhb0WIzKAvMxPVmwzGwt2k7y8RBivIXWzyHuCTgE1ftdGO1vSbBWIc1zjSP1QUaQLlSfRLwWkHJbxOOdoLzmDyrh9V3iJ4LPFfd49cu3tZjZ8qT8gaNJBWsYKfHrXFSlAhKwmrCAZejUBHEewYE+i/LoVB6OAhS/BX7r7i8Ef2AVxGPX4xEaRL8AnO/uM3L7h5CaeRMprvIjSUuLxpHy+A0n1YZl9ZlZkjXDYNJIXxNRMXX4vbP77xMk/Hn/eDRpziw28eZE7w36vC9E+4aRRkZnU2n0nEPa9NkudCWNtS/qnSrDW8BV4Zjx7r6uu5+a8R6ERri/oMHdXWaxqCN2Rxrl1mj/SNLSnTgiVsYHMZZUY42kknTOWjjKvttYQw0hFYgmytv5PwzTTT6Nevksej6OQMbVFC+TCsQEyk3BuNFzNpVE9lxarj1tE7rSTfh/uf8dNS/egm6eh1oR7XsErYpLUv9RnZ2C4OQfQIVQ54XokLKQsUff3UjKi3RjwRqNpspkWKrkGFAdZ8xbEmuRJtKGxYlIAHZAEcPYhC3TWEuYWa9cRHNayXtl581jGGl1xmwqGnkOYexrrdBpgmVmX0YFtr9HJsZ09GFPAi4OlGdtwdWoRnBzuqBgBZ9hMHKaF0NmzeutSQ+EroAD0QiljAP/XFLtNJKUrCU2y0aSFtYOD/wleYymKEgrkvo8oHb+/HsMJo3ANZFWj2+Brv9U9Lv1ja61oLHMbCMkhFOpCNNSpFUXE0kr8oehUH0es6kEWuYgIa0ZOkWwQuHoKSgheETuqTOBy6oQKtz9fTO7CjjJzH5VUgHdoQim0zrIzPk0mkMVO/BzzOwVpEFeCY//IpMoq2/bCZUS5XExqkSPc31N5LSRmY0gvcmGU5kdtuDQko/QRDF4sQSR0AY/KfbpBpNq0iZSjZQRivZC7S4HU+SMH03geTfNtboWCV7+3phGrlsiYDbp5NChpOmErC0JtFDV1C3qLI11JmmWHURvdkzox7oIODcwl7YW56A2/b0Q5XKHI7TCHIkGareUze+LVuCpbXiLWYj64ABSwRpJMQIYRwQB/hnlvSD1uUClUXmhGUZ54jW+h5pI/ZW33D0OHmyENPeR7v5imHxyWu750cArIeGclanl2W5BGis2+56JNGh27fFn7JvbN4caWzkdHrwIX+BOLRy2BOo6fsnMzjOzmBikFO7+MKJH+46ZTW3PdbYVZjbAzL6Oko0/oGWhqhY/CySb40g14EIFK0RPyygNYjMQUk00jJTZdixpQGECadChLJq4LOqbO9PMxqPyprxPtxjScnegxtafkvaLFcY7mdlklPiNMZD0Xt+cSunTILpzVDDUjMUsQAvDAGQiPG5mt5vZZ4P5sTAcgr7IW1pxbE0QTJUZyF+ohky0tXgXTeAACVHc8zSZYiftBIoBgJFEwhG+o9bMoxoevxYJVhymLxOsghkY0gUrIGEBlRe94+55f2kQuvHfQDyS00n9tAkUF4CMtiFGvzwLcMCdVAhlJtF6DpVWoaM11v+hqF012BKFf582s72aExrX9L5j0Iq4bpXv1WqEpHYZ3VY9cHIuQdpUEnBYilRj5QMVZTmspYi0jpmtRGrONaexFnRqh99kYL7YNvBrxPfZsSjocF3Y/gy5kicz2xKNN70J+Gww/6aT01ihsv/FyOzrS0oKW0YCC/Kxsu+mTMu1Cx0mWCFg8Y0WD2wZSyP2p7+Z2cbNHHMeMiF+Y2YTmzmmXQim3zeQUxyTotQDr6EJlxmmhpaQPEZEbfMTSQUr1k5rk954ZSVIZZ3JYyiG7suGgI8kd7MH7X4kcFAubL4S8K4J3wZ+Axzq7sfljlma4jTIqaQRzziyCApcxIP0FhwbBHQs5bWMVaMjNdaPSUOe7cGawF1mdp1VpvgBCxh4Po9WpZtNvOE1gZn1MbNDkE1+CrX9TAvDCVFN3Md5jRXM7Dj8Pp5ixUETqZnWm/IayzLBiveNpdgyUmYGNlEMepyK/MQnwnVvh0y4QSjXtRWwmrsvqD8MGvTtaNGYRtqpXBaMa246zerhPpmAoqJxdLFd6BDBMrN10ESSemAn4AkzOyPfUxPaAbZDDW3/MI0bqhpmNszMvoraG84lJeKvJ54hVxMYfJSYY30v0pt6aESpPZJ0hNEQUo01nHLBirXRsKh2MNZgkGv9MLNtUD7sxLA9ETHYzkTBi7uBTdw9/mynkwYuRpFWU/Qlje611E2emYE17TrvKI11Ci3XALYHfVHH8TNmdlRQ77j7k6j8Zzhwo5ndYGZtohQ2s+XN7Bfopj2D6n3E9uDHUev7WMpZh2JhiL/zkaQ30OqkgrUmabX7UFLNFuem+lGusV4MEbvLkAmY5cM2RmbfucAS7n5yTKsQCGQ2Iw1cDCMdFN6XNJgSRzczXzD7/iahXF+cDmgX6p7HCjZ1c75QrTEC5UK+bGbHuftv3f1WM1sf5cW2B7Y3s4dRqdRf0Er4fpYfCf1KqyBeiF2ojnejOcxHDXfPIn/lPSoTON6L9vVG7RhNpCtzGVnLKqQ3fllfVIxBJa9bG31fecyNckiQshT3pVywHkJF09fmaenc/SpUA7owfB59jjjpO5x0UHh8PVA+9WY4lSqSScD9tW6Q7YgEcd2Gey0E01Dg4u/IN7nRzFZHSduvIbNjNXL1iWb2CXKyh1M7Tf46ihg+EP7OKGlPT2BiFboVCeA80hu/LFiyBDkn3dIRSVCeWxtIWuI0gZz2C0nvshU91lh9S841Ejgafa/H0gaEvNuhYbNs5FDMKlUmWGWL0GJUtPQy1Ni/gjoLlpntjSoEOgtrAr83s/8HfN/dTzGzMxCfxuHIDMrMpX60L6n7HjJNHiIIUwj9V4NzEJFLlmKITbVxpCbdoOi4MaQ3eZnGGhWZmQCzo9b64aT+FaQ3cj9SH2sUGja+bkl6oCXsgcLs75BWkHhgWY6vJzb9Vi85r1FZOLZA5mhNUW+NVdVY0zpgdTRW6GFUn3aZu18aaAA2D48tUKCjNXgL2fKPZI8Sh7sqmNkxqNdqd7SS9i65IceSjuVZnKJmG0259oiFqIxbJC5dSiKCwWeKNWJfigSfWb3kvlkUsLUI2urbKEWzWonwl5lua6BUSx6rkVbA9wL+Eyp6yvq32o26CZaZrY0+VFfCaojM8xEzOwG43t1/hQaVYxpm9ymkEYaGRy90o81EK+fDqI2l5tyFZrYTEvwd3f3mIPhlyc1x5IZrm9mKKCGeF6wxpObbq+4em1QxFTSkJUhlEcFVSNmZhhMEKwjGFch/uY62Yw90f/4MlYjFKAuG3ecahJjHFFJ/0VCFylZhu1rLolnUU2Md2vIhnYZVUdb/0SBg17nwLGkbQofAzHZBkxIPdveM6nkE5YIVU0ZnNXOxYMUaqzC7N0THyioVyjRWawRrHeCMUG1xOQrlH1/6GeKCAAASQElEQVRy/QtFTlsdgXzMuBUmu/xNgCXd/ZKwbzpwe3TcHFIfNROsryNro6Wh821GXcLtISH7+Xqcu8ZYGUWrnjKz42qZSG4LzOz/CGFnd/9l7qnhlCdv4+kh4xExS35fc6ZgHlMoUjGvjrRcrOmGlVzHcFLBmh5KjE5C/u1eVWr2rwFPuvstyD8rq75fBTXC5n2oFUuOm03aOgP6jBtTpymf9cpjdWRFQi0wHbFDvWhmt5jZ7qGSoe4IwZSvohU+npCyMI2VN9fGAz+PjilorKCJ4hKouHP4JzSfHI6bHF8lFazZZrY7Gja4UzU9ccGF+BYVJuRliHwkM8t84sUomnlxlzCouDYWrLdQf9xsiiNWa4aaC5ap27UaFtuugF6IHuxq4FUz27eeb2Zm30EFqBmPeBykaE6wyjRWnNOJW9tHkvolC6ouQlXEKsjfjBfFMsGaQipYTSjZu1tbgxXhGoah7/4nuWr04QTBMrNeZnYYanoEONXd8yH3Mr+rt6cD3bdFpVWXRmViNUNNBSvY1j9r8cDugRHApWZ2bUgw1wzhBjmOIFTu/hyKTsV1bcOJKiBC4e0AihXp40kjX6MpOuUjSUPRQ4B3TOxOZyGyngeBz5ZcR6zFlqc4JXIcyn8d7O63UR0uQMKRH8M0CXjO1Mf3d6SZh6Gk+QL/Ldx7rTU7VwiPug17r7XGOojyvEF3xs7AvWb2sJkdaOKuqBohgncvcpw3CzWNoBsoXnHjifWgKu/no0qBMsHyKBc1EvmTeQxG98DPUIT0D6hyYl1T526GQaQVFYMIGstEHnofcIS7tzknZGZ9QxBpN+CK6LqXQHO7/oBSAwcjrvpfl7DklmmfsrD8cOAOd68bN0rNBMvMmgjFlT0Uq6IV9RUz+6mZTW/rCUwjX+9ADX5/igpYm0jpzBKNhQTr5GjfgBJ/JhbSJtIkay/gOGA5KlUor4T9+Xab/5W0u/cBZgZ/56/Ad909Jg9tESEf9hhK2P8JLWJLmtlOZvYg8oX+h7T7CkioPkXKYTGYtJsZogbGcJ/uT+qT1hS11FgnUp7Z72kYjvqJnjKzm0JLQ4sI/tQhqPnyZtJh23M8nU9VprGmk6NxNvHyFXydUOUf+2tl+yYijfD5HL9FpkHzDLPNkc3sAVwPHOjul5cc0xqsj0rQhiKBuQWlPM5BbE+ruPuW7n5L0NJfRvftg9F54sHgGWLz8Bik3auZFNpq1CSPFfqhDqrFuboRDPki25jZZcB33D2Zdhh8g6+jfNGG7j7bzNaj2N4wllSAoNy32QxVT2R+yHjSJr2lKeccXDBxw8yWQVpqbw/EpwFl/UuFMH0Q5jEocbu9u8eMua2Gu19tZvehZG026+r+4HcWEAqkv4hSC7HZN5goBxm0U74SZBzSjLvXuug2Rq0SxEfRtVh1OxK9kGmxl5n9Ba2EN6IAw6loRb4f0WFnPsGqFCvWmyurKfhJZrYpqiHMJ2snkvpXS5P2azURQvQh9H4FcJG7x2NlM0qw/CzgBTWBoQzoyvC5t3P3/GyuqhBYoy5oxaHfQ1qpLIk/nLQLYCJF3/BbwGPufhN1RruFwTQkrq5h6W6CfmjVPRv5MnejEPgSqOUhr1VWpdguPonyspoFPljw6bLhD/kE7tSS1y5FucZ6P4S0f4NM0aNL3nMo8qnyGvQ5MxtjZiehyNwsFHhpt1C1Fma2KkocH0bU9BgqSE6lXLCyJssVUeDj23W/WGqjZb5G90oGdwSuBFZ09x+ENpF1KQrSUhSrwJvTWB+ZcABqPcn8njxN2FRSwRpXcr4m5Ms8hMqEtivprwL5dU9H+7YJ59sa2NXd13P32MepG4IPegEqPbuRNNI3BBhXkq8aiSj0hqIStvvd/U91v2CqNAVD8+LewD6kLK2LMmagFf97HggqQ2/V8Cg0PD+y8SciTRBjHKLpihtFL879P5VUsJpK9o1EEckrUa4pmf0brvVrqBN7c6SBt0ZNl/sQaipLrrPe+B6idtsubC8fPT+QcsKY8SjF8Cv0PX2hPpdXAndv1QOtZAegH3o+WjUaDz3uQYEJgL9E39s6wNW57SHAldExv0JTVLLtzVHkcV7Jez0QvfY2VF2Q33dVtP1ZVOv37RZ+412i97qL0MHb2vuk1g8UefwY2CBsDwX+Fh0zDTir5LXnobrFOajEqsOue6EaK/hPO1JpUS/r0ARFvGahla030oR9STtMexrmolDzZbCgJCeuAl8R3fwZppL6AmMI0T8zOxrlqZoz02PC074l2ueDcK6B6MZaFZG0xO8b40n0e88CnvO0xaRDYSIhuhj4srtnXb5jSSnOBlLO5jsKKYP9PQ3S1BWlgmVmuwFfQiZIRmM8HzX13YOIPV4Oj1e8mc5QE5fgsigBuRxK8k2u4fV3Bh5BOZY/A1/IhCpgDdJC2qWRY51hKqnZ0g84xDSBZWF5sdfJdbsupIxnnJmdiGrizgS+1hoTzt3/WXJtnYJw79wA3ODuedN3DGmz5iCi78E0EGJd4EavPsdWNRYIVviRtkah883D7jeRmXILaiJrU2u1q5DyJcS9nXHfZZGdTmnRaCM+RBG9fyCB2srdd86etMAGlcNaKPmbx5teKVsCCdZfQk5lZ+BzwAZozlRLuMyLAYfxFLkpJqM+uM8if+Ts6KbsFjBxbNyIgjWx1hxDml4YSE7YTFzwtyHt1powfs3RJ1zI3sB3UbTqExRBuQy4xYsTJ9oFV07mR2b2Y9SvcygK1XeF2cFzUDj7Zyg0/nXg5+6e5X4GAZtmB5vZyqQm3TQ0RC2Pp0Lj3hT0/W6F/IZ1aHtU9s5oeypqddkMJT63p/hdPkA3Q1jgr0T5tPNJudjHkFZNDKRiSi9Ppa5wC3e/s57X2xz6mNnBKOl4HsoP/D27meqFYJbMAGaY2enIp4grqjsafVGkc2+0Is5BdYGvolzIahTzJ9sRzLKQRxmAFqq1QxJ1BeRfrYBuhub809bgGZQYfTC83wQ0BmcP1Jf0zWZe1+0EC40oWgdp8GNI6/+WIuUTHIQWmC8js/t+YE9PyWY6Dp0V7SmJ4GyKhK2zI3wtPV5FydVsey71jZK+RmCbAi5FVQeted2bnf2bVnEP7BC+21XC9q3AptExtwBTo337I2Gbj8qsOi2KueCaOvsCoi/IEFXyO628eRqP5h83dfbv2cbffpWwiGyQ2/cginrmj3sJ6J/bnoSS3u8jEp5O/yzu3rXq+1y4knQIWQNtR7f5/sxsPxRN3d8rYXWAZz3n44cescEe6ieDb/kQyg2u6+7Xd9xVLxxdSrAyuIguN0Z00d7Jl9NdUde2iFrAzIaa2aXAJcB/3P3W3HMTSSssRgP/NbM1zewmVElyN7C2i6e/y6CzZhC3iLBSHW1md6IIZb1Gj/ZEzHD3mOu8SyFwbJyPCGH2othYCSqViwlB30OR1xnI1/ysV6jiuhS6pMbKw1XivyrpFIkGmkebO3k7EoHv42YUvVsWpTkGR4dl+zOOkINRGmQu4rpYoasKFXQDwQJwJZo3Rrm2sukRDVTwIYHZt6shcFscjsLoB7v759z9HZSSiJtEl0U5wOMRW++P0GSS5V1dA2Uknl0HnR09qSJ6tBZqa+jsqFtXfVzS2b9RM7/bTqhrehaaAJN/7gvAp3Lbo1C1/3xEvLMPuUhgd3h0C42Vh7vPQFHDmPy+AWGamV1sZl+phvCm1jCzdczsbpSDOx118cZD5FZEQwq2MrOLUKPov4CV3X0Dd7/cUz6Qro3Olux2roJfQqZhZ2uJrvx4HhUN7wAM6cDfZkXkIzkaRDE+7D8XmJI7bjBqT/kwHHsXsFZn31vt/vydfQE1+AG3RsnBzr6Bu8JjTgvPf4LqDY8D1gP61eH32AoNJpiLWmgOiZ6/BXU4fBlVVsxGBQHXImKaTr+nGoJV+bFWQvV9nX1jd+bjERSyPg0lWx9FBcFPU9EG8WMWygP9GNU+NlX5/S+G+p4eR71Sl6Fw+SlBiAagBfCniFTTw9/rUStR71reD13hYeGL6fYILRz7oXGcnTGAuzPxAbCGN8PsGoqExyNOwuloysoq4e+Q6PCnUMDgHuA2T+dN5c+bVenvgQqNZyMBehT5wRciU3Q9pMH+iDTm/Wh8Ts06J7oaeoxgZQjUXqegvq9FBXu5BmW3CUHglkBCtgryi5ZCC9MQpFluBj7nEY+faeTPQ82cei7i3HgGCeptwJ+9q4fIa4geJ1gZwmr6C8Q315PxO881X9YKgZZhSVTp8LK7/7XkmAnI5GtC2moWYnN6oSdro9agxwoWLKBa/jbqWO6J/BvvooRps+ZaA52DbpfHagvc/R13Pwpl8X+FTJuegnnAYQ2h6pro0RorhpmtgfyvzTr7WtqJV1GHbDxxo4EugroIViCjXw45sXNRfqXs/2y7movIaNZa+xgIPOPu/w6V1SezcEakropbETtUPCe4gS6EegnWN+mas7IW+CSBtGQ7NCUlY3vtypiL/MVTfFEyM7op6iVYfybHaFRnzEYC8y6aE/Vu9HgbzXz6L3CPp/zeWVPdF1GSsyvyHjqwh7v/usUjG+gSqLlgmdkAdIMv1sqXOGIgmtnCIxaYmcC7XsPizEBTthtKMq9aq/O2E8+j2Vt1me7eQH1QD8FaHBV8vo84t2dHf/P/f4SEIyHo70wEM/EbwAl0Xpj+XlQCdH1X+34aaBn1EKwd0JDnLWp64k6AmS2JmGp3Qn1gZSNDa4lHUUX4Nd7JvOkNtA/1EKyd0LTADdz9kZqevBMRqgx2REK2CbXjC3FU8vNNd3+4pYMb6B5ot2CF2rxNEN/7pohcsg/wLXc/qZXnmIiqnAeispiPgefLymi6AgLh/rYomrg6KmxtTbLd0dCB+8LjEeDpuA6vge6Pdq26YXD1ZaiVOkbZtMD8a/sgfvRdEefB31Dh5ghEezXNzP6JRo/+0jUZsUvA3WcCl4dHxuu+MipiHY4KWAeHv2+i1o2ngX96nem7G+gaaK85cyzlQgUpndUChBvxt6jl4CzgM3HCM2ixzYBDgMPNbO2uJFx5uPuHqBXi/s6+lga6BtplCprZEsicGVry9JvAhLjKOcyD/RNqftve3Z9v4T3GINqru91916ovtoEGOhDtKsINQnFKM0+PpjwX9GnU87NeS0IV3uMNNO18FzP7arXX2kADHYlaVLefh4INZXi7ZN8GaFRQUgHRHNz9BuTPHB1yTA000KXR7pvUNa3wt808HVMEg+YelU2IbwnnIb9tqype20ADHYparf7NCVah7i4Mm16V6gTrflTzd1gIfjTQQJdFrZKctyFCk5h/ewphCmHAOuE9m+NKaBbuPt/Mbkasqe+Y2f1oVtL7aFriMBTqHp77fwCiJ97B3WMK4wYaqBtqIlju/rGZ/QHYPXpqUrS9HvCWa0xPNbgJCVY/lJQuw8tout9jSNifawhVAx2NWo7x+S2pYI2NtoeRDmtuC17O/T8HVTE8QmWq/T/cvSxg0kADHYpaCtaNaNTl4rl9sWD1QoOYq0XmE2YT7Rda3dFAA52FmoWuA2fcT6LdY6JtI/XD2oKsuvzVhlA10JVR65zQpYg9KEOZxhpgZktVef7sehut6Q10adRUsEJxaj53FQtWVsW9XpVvkQVDnq3y9Q000CGoRxXD2bn/Rwca4wwZt/g2VZ57J1Tl8Y8qX99AAx2CegjW1bn/+6I2kAxPh7+7m1k8EX2hMLOlUQ/U/Ys6fXEDXR/1EKwsf5QhH8DIBKsXcGogb2kRoWIjm7D+u1pcZAMN1BM1F6xAfPK33K7RuefepTImcxvgGjNbKJuTma2LfKpPhV3X1+xiG2igTqhXpXi+4S9mOfoiSu4C7Azca2ZfNLNh+YPMbLyZ/RDlx8aF3X9395fqccENNFBLdIRgFZLQ7v4P4Ae5XWsAFwGvm9m/zexFM3sDjYP5FsUO5Yvqc7kNNFBb1LLyIo88O1PZe/wIzVT6CpWk72KoAbIMHwIHVzNcrYEGOgP10livUml+TATL3ee6+5GImen1Fs71L2DdhlA10J1QF8EKpP1Z232zWtHdb0VTSY4Cnoye/gfwHWAtd3+8HtfZQAP1Qr1MQVAkb7mW3iNUa/wU+GkYzzkfmOPu79fx2hpooK6op2A919b3aMx8aqCnoJ7ELPeEv/UU3gYa6JKop2D9KfxtCFYDixzqJljBd/oQ6F+v92igga6KenP0PUnXnJDYQAN1Rb3NtDMQz0UDDSxS+P/SuwD4nierXwAAAABJRU5ErkJggg==" />
		<h1>Bismark Processing Report</h1>
		<div class="subtitle">
			<h3>{{filename}}</h3>
			<p>Data processed at {{time}} on {{date}}</p>
		</div>
	</div>
	<hr id="header_hr">
	
	<h2>Alignment</h2>
	<table class="data">
		<tbody>
			<tr>
				<th>{{sequences_analysed_in_total}}</th>
				<td>{{total_sequences_alignments}}</td>
			</tr>
		</tbody>
		<tbody>
			<tr>
				<th>{{unique_seqs_text}}</th>
				<td>{{unique_seqs}}</td>
			</tr>
			<tr>
				<th>{{no_alignments_text}}</th>
				<td>{{no_alignments}}</td>
			</tr>
			<tr>
				<th>{{multiple_alignments_text}}</th>
				<td>{{multiple_alignments}}</td>
			</tr>
			<tr>
				<th>Genomic sequence context not extractable (edges of chromosomes)</th>
				<td>{{no_genomic}}</td>
			</tr>
		</tbody>
	</table>
	<div id="alignment" class="plot"></div>

	<hr>
	
	
	<h2>Cytosine Methylation</h2>
	<table class="data">
		<tbody>
			<tr>
				<th>Total C's analysed</th>
				<td>{{total_C_count}}</td>
			</tr>
		</tbody>
		<tbody>
			<tr>
				<th>Methylated C's in CpG context</th>
				<td>{{meth_CpG}}</td>
			</tr>
			<tr>
				<th>Methylated C's in CHG context</th>
				<td>{{meth_CHG}}</td>
			</tr>
			<tr>
				<th>Methylated C's in CHH context</th>
				<td>{{meth_CHH}}</td>
			</tr>
			{{meth_unknown}}
		</tbody>
		<tbody>
			<tr>
				<th>Unmethylated C's in CpG context</th>
				<td>{{unmeth_CpG}}</td>
			</tr>
			<tr>
				<th>Unmethylated C's in CHG context</th>
				<td>{{unmeth_CHG}}</td>
			</tr>
			<tr>
				<th>Unmethylated C's in CHH context</th>
				<td>{{unmeth_CHH}}</td>
			</tr>
			{{unmeth_unknown}}
		</tbody>
		<tbody>
			<tr>
				<th>Percentage methylation (CpG context)</th>
				<td>{{perc_CpG}}%</td>
			</tr>
			<tr>
				<th>Percentage methylation (CHG context)</th>
				<td>{{perc_CHG}}%</td>
			</tr>
			<tr>
				<th>Percentage methylation (CHH context)</th>
				<td>{{perc_CHH}}%</td>
			</tr>
			{{perc_unknown}}
		</tbody>
	</table>
	<div id="methylation_context" class="plot"></div>
	
	<hr>

	<h2>Alignment to Individual Bisulfite Strands</h2>
	<table class="data">
		<tbody>
			<tr>
				<th>OT</th>
				<td>{{number_OT}}</td>
				<td>original top strand</td>
			</tr>
			<tr>
				<th>CTOT</th>
				<td>{{number_CTOT}}</td>
				<td>complementary to original top strand</td>
			</tr>
			<tr>
				<th>CTOB</th>
				<td>{{number_CTOB}}</td>
				<td>complementary to original bottom strand</td>
			</tr>
			<tr>
				<th>OB</th>
				<td>{{number_OB}}</td>
				<td>original bottom strand</td>
			</tr>
		</tbody>
	</table>
	<div id="alignment_context" class="plot"></div>

	<hr>
	
	<div id="bm_deduplication" style="display:none;">

		<h2>Deduplication</h2>
		<table class="data">
			<tbody>
				<tr>
					<th>Alignments analysed</th>
					<td>{{seqs_total_duplicates}}</td>
				</tr>
				<tr>
					<th>Unique alignments</th>
					<td>{{unique_alignments_duplicates}}</td>
				</tr>
				<tr>
					<th>Duplicates removed</th>
					<td>{{duplicate_alignments_duplicates}}</td>
				</tr>
			</tbody>
			<tbody>
				<tr>
					<td colspan="2" style="text-align:left;">Duplicated alignments were found at <strong>{{different_positions_duplicates}}</strong> different positions</td>
				</tr>
			</tbody>
		</table>
		<div id="deduplication_plot" class="plot"></div>
		<hr>
	</div>
	
	<div id="bm_splitting" style="display:none;">
		
		<h2>Cytosine Methylation after Extraction</h2>
		<table class="data">
			<tbody>
				<tr>
					<th>Total C's analysed</th>
					<td>{{total_C_count_splitting}}</td>
				</tr>
			</tbody>
			<tbody>
				<tr>
					<th>Methylated C's in CpG context</th>
					<td>{{meth_CpG_splitting}}</td>
				</tr>
				<tr>
					<th>Methylated C's in CHG context</th>
					<td>{{meth_CHG_splitting}}</td>
				</tr>
				<tr>
					<th>Methylated C's in CHH context</th>
					<td>{{meth_CHH_splitting}}</td>
				</tr>
				{{meth_unknown_splitting}}
			</tbody>
			<tbody>
				<tr>
					<th>Unmethylated C's in CpG context</th>
					<td>{{unmeth_CpG_splitting}}</td>
				</tr>
				<tr>
					<th>Unmethylated C's in CHG context</th>
					<td>{{unmeth_CHG_splitting}}</td>
				</tr>
				<tr>
					<th>Unmethylated C's in CHH context</th>
					<td>{{unmeth_CHH_splitting}}</td>
				</tr>
				{{unmeth_unknown_splitting}}
			</tbody>
			<tbody>
				<tr>
					<th>Percentage methylation (CpG context)</th>
					<td>{{perc_CpG_splitting}}%</td>
				</tr>
				<tr>
					<th>Percentage methylation (CHG context)</th>
					<td>{{perc_CHG_splitting}}%</td>
				</tr>
				<tr>
					<th>Percentage methylation (CHH context)</th>
					<td>{{perc_CHH_splitting}}%</td>
				</tr>
				{{perc_unknown_splitting}}
			</tbody>
		</table>
		<div id="dedup_methylation_context" class="plot"></div>
		<hr>
	</div>
	
	<div id="bm_nucleotide" style="display:none;">
		<h2>Nucleotide Coverage</h2>
		<table class="data" id="nucleotide_coverage_table">
			<thead>
				<tr><th>Nucleotide Class</th> <th>Counts Sample</th> <th>Counts Genome</th><th>% in Sample</th> <th>% in Genome</th> <th>fold coverage</th></thead>
			<tbody>
				<tr><th>A</th>	<td>{{nuc_A_counts_obs}}</td> <td>{{nuc_A_counts_exp}}</td> <td>{{nuc_A_p_obs}}</td>  <td>{{nuc_A_p_exp}}</td>	<td>{{nuc_A_coverage}}</td></tr>
				<tr><th>T</th>	<td>{{nuc_T_counts_obs}}</td> <td>{{nuc_T_counts_exp}}</td> <td>{{nuc_T_p_obs}}</td>  <td>{{nuc_T_p_exp}}</td>  <td>{{nuc_T_coverage}}</td></tr>
				<tr><th>C</th>	<td>{{nuc_C_counts_obs}}</td> <td>{{nuc_C_counts_exp}}</td> <td>{{nuc_C_p_obs}}</td>  <td>{{nuc_C_p_exp}}</td>  <td>{{nuc_C_coverage}}</td></tr>
				<tr><th>G</th>	<td>{{nuc_G_counts_obs}}</td> <td>{{nuc_G_counts_exp}}</td> <td>{{nuc_G_p_obs}}</td>  <td>{{nuc_G_p_exp}}</td>	<td>{{nuc_G_coverage}}</td></tr>
			</tbody>
			<tbody>
				<tr><th>AC</th>	<td>{{nuc_AC_counts_obs}}</td> <td>{{nuc_AC_counts_exp}}</td> <td>{{nuc_AC_p_obs}}</td> <td>{{nuc_AC_p_exp}}</td> <td>{{nuc_AC_coverage}}</tr>
				<tr><th>CA</th>	<td>{{nuc_CA_counts_obs}}</td> <td>{{nuc_CA_counts_exp}}</td> <td>{{nuc_CA_p_obs}}</td> <td>{{nuc_CA_p_exp}}</td> <td>{{nuc_CA_coverage}}</tr>
				<tr><th>TC</th>	<td>{{nuc_TC_counts_obs}}</td> <td>{{nuc_TC_counts_exp}}</td> <td>{{nuc_TC_p_obs}}</td> <td>{{nuc_TC_p_exp}}</td> <td>{{nuc_TC_coverage}}</tr>
				<tr><th>CT</th>	<td>{{nuc_CT_counts_obs}}</td> <td>{{nuc_CT_counts_exp}}</td> <td>{{nuc_CT_p_obs}}</td> <td>{{nuc_CT_p_exp}}</td> <td>{{nuc_CT_coverage}}</tr>
				<tr><th>CC</th>	<td>{{nuc_CC_counts_obs}}</td> <td>{{nuc_CC_counts_exp}}</td> <td>{{nuc_CC_p_obs}}</td> <td>{{nuc_CC_p_exp}}</td> <td>{{nuc_CC_coverage}}</tr>
				<tr><th>CG</th>	<td>{{nuc_CG_counts_obs}}</td> <td>{{nuc_CG_counts_exp}}</td> <td>{{nuc_CG_p_obs}}</td> <td>{{nuc_CG_p_exp}}</td> <td>{{nuc_CG_coverage}}</tr>
				<tr><th>GC</th>	<td>{{nuc_GC_counts_obs}}</td> <td>{{nuc_GC_counts_exp}}</td> <td>{{nuc_GC_p_obs}}</td> <td>{{nuc_GC_p_exp}}</td> <td>{{nuc_GC_coverage}}</tr>
				<tr><th>GG</th>	<td>{{nuc_GG_counts_obs}}</td> <td>{{nuc_GG_counts_exp}}</td> <td>{{nuc_GG_p_obs}}</td> <td>{{nuc_GG_p_exp}}</td> <td>{{nuc_GG_coverage}}</tr>
				<tr><th>AG</th>	<td>{{nuc_AG_counts_obs}}</td> <td>{{nuc_AG_counts_exp}}</td> <td>{{nuc_AG_p_obs}}</td> <td>{{nuc_AG_p_exp}}</td> <td>{{nuc_AG_coverage}}</tr>
				<tr><th>GA</th>	<td>{{nuc_GA_counts_obs}}</td> <td>{{nuc_GA_counts_exp}}</td> <td>{{nuc_GA_p_obs}}</td> <td>{{nuc_GA_p_exp}}</td> <td>{{nuc_GA_coverage}}</tr>
				<tr><th>TG</th>	<td>{{nuc_TG_counts_obs}}</td> <td>{{nuc_TG_counts_exp}}</td> <td>{{nuc_TG_p_obs}}</td> <td>{{nuc_TG_p_exp}}</td> <td>{{nuc_TG_coverage}}</tr>
				<tr><th>GT</th>	<td>{{nuc_GT_counts_obs}}</td> <td>{{nuc_GT_counts_exp}}</td> <td>{{nuc_GT_p_obs}}</td> <td>{{nuc_GT_p_exp}}</td> <td>{{nuc_GT_coverage}}</tr>
				<tr><th>TT</th>	<td>{{nuc_TT_counts_obs}}</td> <td>{{nuc_TT_counts_exp}}</td> <td>{{nuc_TT_p_obs}}</td> <td>{{nuc_TT_p_exp}}</td> <td>{{nuc_TT_coverage}}</tr>	
				<tr><th>TA</th>	<td>{{nuc_TA_counts_obs}}</td> <td>{{nuc_TA_counts_exp}}</td> <td>{{nuc_TA_p_obs}}</td> <td>{{nuc_TA_p_exp}}</td> <td>{{nuc_TA_coverage}}</tr>
				<tr><th>AT</th>	<td>{{nuc_AT_counts_obs}}</td> <td>{{nuc_AT_counts_exp}}</td> <td>{{nuc_AT_p_obs}}</td> <td>{{nuc_AT_p_exp}}</td> <td>{{nuc_AT_coverage}}</tr>
				<tr><th>AA</th>	<td>{{nuc_AA_counts_obs}}</td> <td>{{nuc_AA_counts_exp}}</td> <td>{{nuc_AA_p_obs}}</td> <td>{{nuc_AA_p_exp}}</td> <td>{{nuc_AA_coverage}}</tr>
			</tbody>
		</table>
		<div id="nucleotide_coverage" class="plot" style="height: 600px;"></div>
		<hr>
	</div>

	<div id="bm_mbias" style="display:none;">
		<h2>M-Bias Plot</h2>
		<div id="m_bias_1" class="fullWidth_plot"></div>

		<div id="m_bias_2" class="fullWidth_plot" style="display:none;"></div>
		<hr>
	</div>

	
	<footer>
		<a style="float:right;" href="http://www.bioinformatics.babraham.ac.uk/"><img alt="Babraham Bioinformatics" src="data:image/png;base64,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" /></a>
		<p>Analysis produced by <a href="http://www.bioinformatics.babraham.ac.uk/projects/bismark/"><strong>Bismark</strong></a> (version {{bismark_version}}) - a tool to map bisulfite converted sequence reads and determine cytosine methylation states</p>
		<p>Report graphs rendered using <a href="http://jquery.com/">jQuery</a> and <a href="http://www.highcharts.com/">Highcharts</a>. Page design by <a href="http://phil.ewels.co.uk/">Phil Ewels.</a></p>
	</footer>

</div>
</body>
</html>

