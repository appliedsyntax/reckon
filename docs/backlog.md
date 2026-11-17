# Backlog: statistics

Requested by a user who compares laboratory methods and does not want to write Python for it.

Size is a rough guide: **S** is a day or less, **M** is a few days, **L** is a week or more.

## Decisions already made

- **Scope.** The calculator gets the statistics most people use: lists, descriptive statistics and a straight-line fit. The laboratory methods further down are parked. If they are built, they go in the command-line tool only, not the apps.
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

## Next, if wanted

- [ ] **More descriptive statistics.** Quantiles, smallest and largest value. `min` is already the unit "minutes", so these need other names. (S)
- [ ] **Standard errors and confidence intervals for the line fit.** For slope and intercept. (S)
- [ ] **Quadratic and cubic regression.** Polynomial fits of degree 2 and 3, validated against the NIST polynomial data sets. Needs a way to show three or four coefficients from one line. (M)
- [ ] **Arithmetic on lists.** `weights * 2`, `weights in lb`. Today a list can only be passed to a function. (S)
- [ ] **Data in and results out on the command line.** Read a CSV file or standard input and print results as JSON, so scripts and AI assistants can run every method without the app. (M)

## Parked: laboratory methods (command-line tool only)

- [ ] **Deming regression.** A line fit that allows measurement error in both methods, given the ratio of their error variances; confidence intervals by jackknife. (M)
- [ ] **Passing-Bablok regression.** A fit based on the median of pairwise slopes that resists outliers, with confidence intervals for slope and intercept. (M)
- [ ] **Bland-Altman analysis.** Mean difference (bias) and limits of agreement between two methods, with their confidence intervals. (S)
- [ ] **Confusion matrix measures.** From true and predicted labels: sensitivity, specificity, F1, positive and negative percent agreement (PPA, NPA) and overall agreement. Cohen's kappa is a likely addition but was not asked for. (S)
- [ ] **Logistic regression.** Fit a yes/no outcome against one or more inputs, then report the confusion matrix measures at a chosen cut-off. (L)
- [ ] **Chart output.** An SVG of a scatter plot with the fitted line, and of a Bland-Altman plot, saved to a file by the command-line tool. (M)

## Not planned

- **Charts inside the apps.** Each platform draws differently, so this is three pieces of work for a feature few users of a calculator need.
- **A general statistics package in the style of Wolfram Alpha.** Too broad for a calculator. New methods are added one at a time when someone asks for them.
