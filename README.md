# Forest Fire Simulation

A scalable, cross-language forest fire simulation for research, visualization, and experimentation.

---

## 🚀 Key Features

- **Realistic Fire Modeling:** Simulates wildfire spread across customizable landscapes, including wind and regrowth.
- **Functional Data Generation:** The data generation module is written in Scala using a functional programming approach.
- **High-Performance Simulation:** Core simulation written in Rust for speed and scalability.
- **Streaming Output:** Simulation results are streamed in NDJSON format for easy analysis and visualization.

---

## 🌲 Visual Examples

### Simulation Screenshot

![image](https://github.com/user-attachments/assets/a412cc62-50cf-497a-a08c-8e963d1b7fe8)
  
*An example snapshot of the forest fire simulation in progress.*

---

### Fire Spread Graph

![image](https://github.com/user-attachments/assets/47fb803e-2d41-4bcf-b321-e7faa7e38053)

*Graphs showing tree states, grass trees, and other simulation statistics over time.*

---

### Animated Simulation GIF

![Simulation Animation](https://github.com/user-attachments/assets/820da1c1-98a1-4be0-a133-a01602e2edf5)


*An animated GIF demonstrating the forest fire spreading across the landscape and the graphs generating simultaneously.*

---

## 🛠 Prerequisites

Ensure you have the following installed:

- **Rust** ([Install Rust](https://www.rust-lang.org/tools/install))
- **Scala** and **SBT**
  - Java Development Kit (JDK) 8 or higher  
  - [Install SBT](https://www.scala-sbt.org/download.html)

---

## ⚙️ Installation & Setup

1. **Clone the repository:**
    ```bash
    git clone https://github.com/MadeInShineA/forest-fire-simulation.git
    cd forest-fire-simulation
    ```

2. **Build the Scala data generation module (functional programming):**
    ```bash
    cd data-generation
    sbt package
    cd ..
    ```

3. **Build the Rust simulation:**
    ```bash
    cargo build --release
    ```

---

## 🚦 Running the Simulation

Run these steps in order for correct data flow:

1. **Generate input data:**  
   The Scala module generates the forest landscape data. (The JAR is built with `sbt package`, and the Rust simulation expects this data in the appropriate location.)

2. **Run the Rust simulation:**  
   From the project root:
   ```bash
   cargo run --release
   ```
   By default, the Rust program looks for input data in the `assets/` directory and streams output to `assets/simulation_stream.ndjson`.

---

## 📝 Configuration

- **Simulation parameters:**  
  - Customize data generation by editing the functional Scala source in `data-generation/Main.scala` and `data-generation/Grid.scala`.
  - Adjust simulation parameters for the Rust core in its configuration or in the Rust source.

- **Input/Output:**  
  - Input and output files are located in the `assets/` directory.  
  - Simulation steps are output as NDJSON to `assets/simulation_stream.ndjson`.

---

## 🤝 Contributing

Contributions are welcome!

1. **Fork** the repository
2. **Create a branch:**
    ```bash
    git checkout -b feature/your-feature-name
    ```
3. **Make and test your changes**
4. **Commit:**
    ```bash
    git commit -m "Describe your changes"
    ```
5. **Push:**
    ```bash
    git push origin feature/your-feature-name
    ```
6. **Submit a Pull Request**

---

## 📄 License

*No license specified. All rights reserved by [MadeInShineA](https://github.com/MadeInShineA).*

---

## 🙏 Acknowledgments

Special thanks to the open-source community for tools and inspiration.

---

*Note: The Scala data generation module is written in a functional programming style, leveraging immutability, case classes, and functional transformations for clear, maintainable code.*
