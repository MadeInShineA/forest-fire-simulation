# Forest Fire Simulation: Model Variables and Wind Formulation

## Overview

This document details the rationale behind variable choices in the forest fire simulation model implemented in Scala. Where possible, parameter values and formulas are justified by reference to primary literature in forest fire ecology and wildfire modeling. Citations to individual sources are annotated by section and parameter.

---

## 1. Cell Types and States

The model landscape is discretized into cells, each of which may be in one of several biologically-inspired states:

| Symbol      | State                | Description                                      |
| ----------- | -------------------- | ------------------------------------------------ |
| W           | Water                | Non-flammable, acts as a firebreak               |
| G           | Grass                | Quickly flammable ground cover                   |
| s           | Sapling              | Young tree, transitional state                   |
| y           | YoungTree            | Older than sapling, not mature                   |
| T           | Tree                 | Mature, primary fuel for crown fire              |
| \*,\*\*,\*\*\*    | BurningTree (1/2/3)  | Stages of burning for Tree                       |
| !           | BurningSapling       | Sapling on fire                                  |
| &,@         | BurningYoungTree     | Young tree on fire (two stages)                  |
| +           | BurningGrass         | Grass on fire                                    |
| TH          | Thunder              | Lightning strike (causes instant ignition)       |
| A           | BurnedTree                  | Post-fire residue from trees                     |
| -           | BurnedGrass          | Post-fire residue from grass                     |

Cell states were designed for simulation expressivity rather than direct empirical mapping, but post-fire stages (BurnedTree, BurnedGrass) and developmental stages (Sapling, YoungTree) reflect ecological succession concepts. See [Bond & van Wilgen 1996, 1], [Johnstone et al. 2016, 2].

---

## 2. Regrowth Timing Parameters

All timing parameters are expressed in days (model steps).

| Parameter                | Value | Justification & Source                                                        |
| ------------------------ | ----- | ----------------------------------------------------------------------------- |
| `burnedTreeRegrowSteps`         | 300   | ~10 months; matches time for soil & seeds to recover after severe burn [1,2]  |
| `burnedGrassRegrowSteps` | 15    | ~2 weeks; grass quickly recolonizes post-burn [1]                             |
| `saplingGrowSteps`       | 60    | ~2 months; rapid establishment phase (see [1], Table 6.1)                     |
| `youngTreeGrowSteps`     | 180   | ~6 months; trees reach maturity after establishment [2]                       |

**Sources for this section:**
- [1] Bond, W.J., & van Wilgen, B.W. (1996). *Fire and Plants*. Cambridge University Press.
- [2] Johnstone, J.F. et al. (2016). "Changing disturbance regimes, ecological memory, and forest resilience." Front. Ecol. Environ., 14(7):369-378.

---

## 3. Ignition Probabilities

Each day, a living cell may ignite from a burning neighbor, with probability **per neighbor**:

| Parameter         | Value | Justification & Source                                             |
| ----------------- | ----- | ------------------------------------------------------------------ |
| `treeIgniteProb`  | 0.02  | 2% daily chance per burning neighbor, moderate fuel moisture [3,4] |
| `grassIgniteProb` | 0.08  | 8% daily chance per burning neighbor, reflecting higher flammability and spread [3]        |

Probabilities increase for young vegetation (see model code). These values are inspired by statistical rates from the Rothermel model and wildfire simulation systems.

**Sources for this section:**
- [3] Andrews, P.L. (2018). "The Rothermel surface fire spread model and associated developments: A comprehensive explanation." RMRS-GTR-371.
- [4] Finney, M.A. (1998). "FARSITE: Fire Area Simulator—model development and evaluation." USDA Forest Service.

---

## 4. Wind Effect Formulation

**Wind is a critical driver of fire spread.**

### A. Wind Spread Amplifier (Sigmoid Law)

To capture the nonlinear acceleration of fire spread by wind (see [5,6]), a *sigmoid* function is used to model the transition from low to high spread at a critical windspeed:

$\displaystyle
\text{sigmoid}(x) = \frac{1}{1 + e^{-x}}
$

The wind effect multiplier is then:

$\displaystyle
\text{windAmplifier}(w, s, m, M) = 1 + (M - 1) \cdot \text{sigmoid}\left( s \cdot (w - m) \right)
$

Where:
- $w$ = wind strength (km/h)
- $s$ = wind steepness parameter
- $m$ = wind midpoint (critical wind speed, km/h)
- $M$ = windMaxMult (maximum amplification at high wind)

Typical values:
- $s = 0.4$ (steepness of transition) [6]
- $m = 20.0$ km/h (midpoint, rapid spread threshold) [5]
- $M = 7.0$ (maximum spread rate multiplier) [5]

### B. Directional Adjustment

Fire is most likely to spread in the direction of wind. For each neighbor, the probability of ignition is:

$\displaystyle
P_\text{adj} = P_\text{base} \cdot (1 + \text{alignment}) \cdot \text{windAmplifier}(...)
$

Where:
- $P_\text{base}$ = base ignition probability
- $\text{alignment} = \cos(\theta) =$ Cosine of the angle between wind and neighbor direction; ranges from $-1$ (opposite), $0$ (perpendicular), to $+1$ (same direction).
- $\text{windAmplifier}$ as above

Cells directly downwind ($\text{alignment} = 1$) receive up to double the base probability before wind amplification [5,6].

### C. Fire Jumping (Spotting)

Rare, wind-driven "spotting" events allow fire to jump over one or more cells, simulating embers igniting fuels ahead of the main fire front ([5,6]). Spotting is attempted only when wind is enabled, and the cell did not ignite via adjacent burning neighbors.

For each unburned, flammable cell, fire jumping is checked at discrete distances $d \in \{2, 3, 4\}$ along the wind direction. If any cell at those distances in the wind’s path is burning, the chance of ignition by spotting is:

$P_\text{jump}(d, w) = \text{fireJumpBaseChance} \times \text{fireJumpWindMult}(w) \div (d \times \text{fireJumpDistFactor})$

Where:

- $\text{fireJumpBaseChance} = 0.002$ (base daily probability of spotting)
- $d$ = jump distance (in cells, e.g., 2, 3, 4)
- $\text{fireJumpDistFactor} = 3.0$ (scaling: longer jumps are less probable)
- $\text{fireJumpWindMult}(w)$ is a wind-driven amplification factor:


$\text{fireJumpWindMult}(w) = 1 + (\text{fireJumpMaxMult} - 1) \cdot \text{sigmoid}(s \cdot (w - (m + 2)))$

- $w$ = wind strength (km/h)
- $s = 0.4$ (wind steepness)
- $m = 20.0$ (wind midpoint, shifted +2 for jumping threshold)
- $\text{fireJumpMaxMult} = 5.0$ (max wind-driven jump multiplier)

A fire jump occurs if a burning cell exists at distance $d$ in the wind direction and a random draw is less than $P_\text{jump}(d, w)$. This mechanism lets high wind conditions trigger rapid, discontinuous fire advance, bypassing typical firebreaks.

**Sources for this section:**  
- [5] Albini, F.A. (1976). "Estimating Wildfire Behavior and Effects."  
- [6] Cheney, N.P., Gould, J.S., & Catchpole, W.R. (1998). "Prediction of fire spread in grasslands."

---

## 5. Regeneration and Succession Probabilities

- $\text{BurnedTreeToTreeProb} = 0.03$ (3% chance per day for burned tree to regrow as a sapling, post-delay) [1]
- $\text{burnedGrassToGrassProb} = 0.95$ (95% chance per day for burned grass to regrow, post-delay) [1]

These values ensure fast recolonization for grass and slower, probabilistic return of trees, consistent with successional dynamics (see [1], chapters 5–6).

---

## 6. References

1. Bond, W.J., & van Wilgen, B.W. (1996). *Fire and Plants*. Cambridge University Press.
2. Johnstone, J.F. et al. (2016). "Changing disturbance regimes, ecological memory, and forest resilience." Front. Ecol. Environ., 14(7):369–378.
3. Andrews, P.L. (2018). "The Rothermel surface fire spread model and associated developments: A comprehensive explanation." RMRS-GTR-371.
4. Finney, M.A. (1998). "FARSITE: Fire Area Simulator—model development and evaluation." USDA Forest Service.
5. Albini, F.A. (1976). "Estimating Wildfire Behavior and Effects." USDA Forest Service.
6. Cheney, N.P., Gould, J.S., & Catchpole, W.R. (1998). "Prediction of fire spread in grasslands." Int. J. Wildland Fire, 8(1):4–13.

