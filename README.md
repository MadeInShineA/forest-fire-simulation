# Forest Fire Simulation

A scalable, cross-language, probabilistic wildfire simulation engine for research, visualization, and experimentation.

---

## ⚡ Installation & Quick Start

**Requirements:**

* **Rust** ([Install Rust](https://www.rust-lang.org/tools/install))
* **Scala** with **SBT**

  * Java Development Kit (JDK) 8+
  * [Install SBT](https://www.scala-sbt.org/download.html)
* **Python 3** (for data visualization, optional)

### 1. Clone the repository

```bash
git clone https://github.com/MadeInShineA/forest-fire-simulation.git
cd forest-fire-simulation
```

### 2. Build the Scala data generator

```bash
cd data-generation
sbt package
cd ..
```

### 3. Build the Rust simulation core

```bash
cargo build --release
```

---

## 🚦 Running the Simulation

1. **Generate input data**
   The Scala module generates the forest landscape and initial conditions. (The JAR is built with `sbt package`; output will be in the correct location for the Rust simulation.)

2. **Run the Rust simulation**

   From the project root:

   ```bash
   cargo run --release
   ```

---

## 📊 Running Data Visualization

To reproduce the wind strength analysis and generate summary plots:

1. **Ensure you have Python 3 and the required libraries installed:**
   You may wish to use a virtual environment.

   ```bash
   cd data-visualization
   pip install -r requirements.txt
   ```

2. **Run the visualization python file**

   ```bash
   python main.py
   ```

   This will generate the summary plots inside `res/fire_metrics_vs_wind_strength_averaged.png`

---

## 🚀 Key Features

* **Rich Cell-Based Model:**
  Supports detailed vegetation states including water, grass, tree, sapling, young tree, ash, and dynamic burning stages.
* **Functional Scala Landscape Generation:**
  Initial landscape, regrowth, and ignition logic written in idiomatic functional Scala.
* **Fast Rust Simulation Core:**
  High-performance engine for scalable, streaming fire spread simulation.
* **Streaming Output:**
  Each simulation step is output as NDJSON for real-time or batch visualization and analysis.

---

## 🎥 Camera Controls

The 3D visualization features a **first-person "fly camera"** system, giving you full control to explore the simulation from any angle or altitude.

### Controls

* **Move:**

  * **W/A/S/D:** Forward / Left / Backward / Right (relative to camera view)
  * **Q / E:** Down / Up

* **Look Around (Rotate):**

  * **Hold Left Mouse Button** and drag mouse to rotate camera orientation (yaw and pitch).

* **Zoom:**

  * **Scroll Mouse Wheel** to move the camera forward/backward along its current view direction (smooth zoom).

* **Speed:**

  * Movement speed is consistent; hold keys for continuous flight.

* **Pause/Resume Simulation:**

  * **Spacebar** toggles pause/playback.

* **Interaction:**

  * When the UI sidebar is active or you're interacting with controls/graphs, camera movement is temporarily disabled to avoid conflicts.

---

## 🌳 Vegetation & Fire Model

The landscape is modeled as a grid of cells, each of which may be:

* **Water:** Unburnable, no regrowth.
* **Grass:** Burns rapidly, recovers quickly.
* **Tree → Sapling → Young Tree:** Trees progress through growth stages, burn in several steps, and regrow after fire as saplings or grass.
* **Burned Tree, Burned Grass:** Track post-fire recovery for trees and grass.
* **Burning States:** Different burning states capture the stages and animation of combustion for trees, saplings, young trees, and grass.
* **Thunder:** Represents ignition points caused by thunder strikes.

Regrowth, ignition probabilities, fire jump (“spotting”), and wind amplification are all parameterized for realistic, tunable fire behavior.

---

## 📄 Model Variables & Scientific Rationale

For a comprehensive explanation of the simulation’s scientific foundations including parameter choices, wind amplification formulas, and direct references to relevant ecological and wildfire modeling literature see:

* [Forest Fire Simulation: Model Variables and Wind Formulation](https://github.com/MadeInShineA/forest-fire-simulation/blob/main/forest_fire_simulation_model.md)

---

## 🌲 Visual Examples

### Simulation Snapshot

![image](https://github.com/user-attachments/assets/ddc16b5c-2091-49d3-a7bb-b463b98525d1)

*Snapshot of the forest fire simulation in progress.*

### Simulation Time-Series

![image](https://github.com/user-attachments/assets/576d7dda-55ae-4503-ab1f-0c6c12347b1e)

*Tracking living, burning, and regrowing cells across time steps.*

### Animated Simulation

[https://github.com/user-attachments/assets/1629e383-d247-4556-a707-08dd01d942c2](https://github.com/user-attachments/assets/1629e383-d247-4556-a707-08dd01d942c2)

*Animated fire spread with wind, thunder, regrowth, and ecosystem recovery.*

---

## 🔥 Model Wind Strength Analysis

![image](https://github.com/user-attachments/assets/8619df19-2714-44e5-bbb8-57eecd2558db)

### Wind Strength and Burn Severity

This figure summarizes how wildfire intensity and landscape impact vary as wind increases, averaged over multiple simulation runs:

* **Left: Max Burned % of Burnable Cells**
  Shows the highest fraction of living vegetation (grass, trees, saplings, young trees) burning simultaneously at any point, per simulation.
* **Right: Final Burned % of Burnable Cells**
  The percentage of all burnable land that remains burned after the fire ends.

**Phase Transition:**
At low wind, fire is limited most of the forest survives.
Above a critical wind speed (near 20 km/h), fire jumps and wind amplification make large-scale burns much more likely, resulting in nearly total loss of vegetation.
This critical transition appears as a rapid, S-shaped (“sigmoid”) jump in the metrics, driven by the model’s wind amplification and probabilistic fire spread.

**Model settings used for this analysis:**

* Grid: 100×100
* Thunder disabled
* Initial ignition: 5% trees, 10% grass
* Wind angle: 0°
* Wind strength: 0–50 km/h (step 1)
* Each point: mean of 5 runs per wind level

---

*Note: The Scala data generation module is written in a functional programming style, leveraging immutability, case classes, and functional transformations for clear, maintainable code.*
