# Backlog: statistics

Requested by a user who compares laboratory methods and does not want to write Python for it.

Size is a rough guide: **S** is a day or less, **M** is a few days, **L** is a week or more.

## Decisions already made

- **Scope.** The calculator gets the statistics most people use: lists, descriptive statistics and a straight-line fit. Method comparison (Passing-Bablok and Deming) was asked for again, since the three fits are read side by side, so it is in the apps too: the functions work like `slope`. The other laboratory methods stay parked.
- **Deming error ratio.** The third argument is x's error variance over y's, as in CLSI EP09 and the R package mcr. Default 1.
- **No AI inside the app.** An AI assistant or a script uses these features through the `reckon` command-line tool.
- **No Python, numpy or scipy.** The engine is Rust with no dependencies and also runs on iOS and Android, so Python cannot be bundled. Each method is written in the engine.
- **How data is entered.** A list in square brackets, kept in a variable: `x = [1, 2, 3]`. Single-list functions also work on the lines above, the way `sum` does.
- **Argument order.** Line-fit functions take x first, then y: `slope(x, y)`. Spreadsheets take y first.

## Done in 0.3.0

- [x] **Lists of numbers.** `[3 kg, 500 g]` holds values in one unit, can be kept in a variable and joined with other lists.
- [x] **Descriptive statistics.** `median`, `stdev`, `variance`, `pstdev`, `pvariance` and `count`, and `sum` and `mean` now accept a list.
- [x] **Linear regression (ordinary least squares).** `slope`, `intercept`, `predict`, `correlation` and `rsquared`, with units carried through.
- [x] **Validation of the linear fit against NIST reference data.** The Norris data set and three numerical-accuracy data sets run as automated tests.
- [x] **Sample sheet for the new features.** "Lists and statistics" is added to the sheets list once, on a new installation or the first start after the update.

## Done on the method-comparison branch (not yet released)

- [x] **Confidence intervals for the line fit.** `slope_ci` and `intercept_ci`, 95%, Student's t; checked against the certified standard deviations of NIST Norris.
- [x] **Passing-Bablok regression.** `pb_slope`, `pb_intercept` and their `_ci` intervals, up to 1,000 points.
- [x] **Deming regression.** `deming_slope`, `deming_intercept` and their `_ci` intervals (jackknife), with an optional error ratio.
- [x] **Validation.** All three against values computed in R by independent code (R's `lm`, Deming as the major axis of rescaled data, Passing-Bablok written from the 1983 paper).
- [x] **Importing data.** File › Import Data reads CSV, tab-separated and Excel `.xlsx`; cells pasted from a spreadsheet become lists; Paste as Lists for a single column. The command line loads a file with `-d` and prints its lines with `--import`.
- [x] **Long lists on one line.** Plain numbers in a list no longer count toward the 400-item line limit, so a line holds up to 10,000 values.
- [x] **Sample sheet "Method comparison".** The three fits side by side on thirty samples.

## Done on the ml-tools branch (not yet released)

- [x] **Units for AI work.** Parameters, tokens and FLOP as counts, FLOPS as a rate, `70B params`, `2T tokens`, and number formats (`bf16`, `fp8`, `int4`, …) in bytes.
- [x] **Functions.** `sigmoid`, `logit`, `softmax`, `logsumexp`, `log2`, `perplexity`.
- [x] **Evaluation statistics.** `accuracy_ci` (Wilson), `mcnemar`, `paired_ci`, `items_needed`, `holm`; checked against R.
- [x] **Errors instead of silence.** An unknown function call, or a near-miss of a unit stuck to a number (`989TFLOPZ`, "did you mean TFLOPS?"), is an error rather than a quietly wrong number. Other prose works as before.
- [x] **Sample sheet "AI and GPU math".** Model memory, KV cache, training compute and days, serving cost, an API bill and a model comparison.

## Next, if wanted

- [ ] **Check against mcr.** Compare Passing-Bablok and Deming with the R package mcr, the usual reference in clinical chemistry, on its example data. (S)
- [ ] **Bias at a decision level.** The difference between the methods at a chosen x, with its interval, for each fit. (S)
- [ ] **Import on the phones.** The engine can already read the files; the apps need a file picker. (M)

- [ ] **More descriptive statistics.** Quantiles, smallest and largest value. `min` is already the unit "minutes", so these need other names. (S)
- [ ] **Quadratic and cubic regression.** Polynomial fits of degree 2 and 3, validated against the NIST polynomial data sets. Needs a way to show three or four coefficients from one line. (M)
- [ ] **Arithmetic on lists.** `weights * 2`, `weights in lb`. Today a list can only be passed to a function. (S)
- [ ] **More evaluation statistics.** Items needed to detect a gap between two models (paired McNemar power), Cohen's kappa for agreement between raters, and the bootstrap. (S)
- [ ] **Results out as JSON on the command line.** Data already goes in with `-d`; printing results as JSON would let scripts read every method's output. (S)

## Parked: laboratory methods

- [ ] **Weighted Deming.** For measurement error that grows with the value. Iterative and harder to validate. (M)
- [ ] **Linearity test for Passing-Bablok.** The cusum test from the original paper. (S)
- [ ] **Bland-Altman analysis.** Mean difference (bias) and limits of agreement between two methods, with their confidence intervals. (S)
- [ ] **Confusion matrix measures.** From true and predicted labels: sensitivity, specificity, F1, positive and negative percent agreement (PPA, NPA) and overall agreement. Cohen's kappa is a likely addition but was not asked for. (S)
- [ ] **Logistic regression.** Fit a yes/no outcome against one or more inputs, then report the confusion matrix measures at a chosen cut-off. (L)
- [ ] **Chart output.** An SVG of a scatter plot with the fitted line, and of a Bland-Altman plot, saved to a file by the command-line tool. (M)

## Not planned

- **Charts inside the apps.** Each platform draws differently, so this is three pieces of work for a feature few users of a calculator need.
- **A general statistics package in the style of Wolfram Alpha.** Too broad for a calculator. New methods are added one at a time when someone asks for them.
