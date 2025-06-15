import subprocess
import json
import time
import os
import matplotlib.pyplot as plt
import psutil

# === Constants ===
PARENT_DIR = os.path.abspath("..")
SIM_SCRIPT = "./run-sim-ndjson.sh"
SIM_FILE = os.path.join(PARENT_DIR, "res/simulation_stream.ndjson")
SIM_CONTROL = os.path.join(PARENT_DIR, "res/sim_control.json")
MAX_WIND_STRENGTH = 50
REPEATS = 5
WIND_STRENGTH_STEP = 1

GRID_WIDTH = 100
GRID_HEIGHT = 100

THUNDER_PCT = 0
FIRE_TREE = 5
FIRE_GRASS = 10
WIND_ENABLED = 1
WIND_ANGLE = 0

burning_symbols = {"*", "**", "***", "+", "!", "&", "@"}
burnable_symbols = {"G", "T", "s", "y"}
burned_symbols = {"A", "-"}

wind_strengths = []
max_burned_percents = []
burn_durations = []
final_burned_percents = []
peak_fire_fronts = []


def write_sim_control_json(
    path,
    thunder_percentage,
    wind_angle,
    wind_strength,
    wind_enabled,
    paused=False,
    step=False,
):
    control = {
        "thunderPercentage": thunder_percentage,
        "windAngle": wind_angle,
        "windStrength": wind_strength,
        "windEnabled": wind_enabled,
        "paused": paused,
        "step": step,
    }
    with open(path, "w") as f:
        json.dump(control, f, indent=2)


def kill_proc_tree(pid):
    try:
        parent = psutil.Process(pid)
        children = parent.children(recursive=True)
        for child in children:
            child.terminate()
        gone, alive = psutil.wait_procs(children, timeout=3)
        for child in alive:
            child.kill()
        parent.terminate()
        try:
            parent.wait(3)
        except psutil.TimeoutExpired:
            parent.kill()
    except Exception as e:
        print(f"Could not kill process tree: {e}")


for wind_strength in range(0, MAX_WIND_STRENGTH + 1, WIND_STRENGTH_STEP):
    print(f"\nWind strength: {wind_strength}/{MAX_WIND_STRENGTH}")
    total_burned = 0.0
    total_duration = 0.0
    total_final_burned = 0.0
    total_peak_front = 0.0

    for run in range(REPEATS):
        print(f"  Repeat {run + 1}/{REPEATS} ...", end="", flush=True)
        if os.path.exists(SIM_FILE):
            os.remove(SIM_FILE)

        write_sim_control_json(
            SIM_CONTROL,
            thunder_percentage=THUNDER_PCT,
            wind_angle=WIND_ANGLE,
            wind_strength=wind_strength,
            wind_enabled=bool(WIND_ENABLED),
        )

        cmd = [
            SIM_SCRIPT,
            str(GRID_WIDTH),
            str(GRID_HEIGHT),
            str(THUNDER_PCT),
            str(FIRE_TREE),
            str(FIRE_GRASS),
            str(WIND_ENABLED),
            str(WIND_ANGLE),
            str(wind_strength),
        ]
        proc = subprocess.Popen(cmd, cwd=PARENT_DIR)

        while not os.path.exists(SIM_FILE):
            time.sleep(0.05)

        last_processed = 1
        frame_counter = 0
        max_percent_burned = 0.0
        peak_fire_front = 0
        done = False
        grid = None
        initial_burnable_count = None
        percent_burned = 0.0  # For reporting in last frame

        while not done:
            with open(SIM_FILE, "r") as f:
                lines = f.readlines()

            new_lines = lines[last_processed:]
            if not new_lines:
                time.sleep(0.05)
                continue

            for line in new_lines:
                try:
                    loaded = json.loads(line)
                except Exception:
                    continue

                # First grid: establish denominator!
                if initial_burnable_count is None:
                    if isinstance(loaded, dict) and "cells" in loaded:
                        grid = loaded["cells"]
                    elif isinstance(loaded, list):
                        grid = loaded
                    else:
                        continue
                    initial_burnable_count = sum(
                        cell in burnable_symbols
                        or cell in burned_symbols
                        or cell in burning_symbols
                        for row in grid
                        for cell in row
                    )
                    if initial_burnable_count == 0:
                        raise RuntimeError("No burnable cells in grid!")
                else:
                    if isinstance(loaded, dict) and "cells" in loaded:
                        grid = loaded["cells"]
                    elif isinstance(loaded, list):
                        grid = loaded
                    else:
                        continue

                frame_counter += 1
                burned = sum(cell in burned_symbols for row in grid for cell in row)
                percent_burned = 100.0 * burned / initial_burnable_count
                max_percent_burned = max(max_percent_burned, percent_burned)
                burning_now = sum(
                    cell in burning_symbols for row in grid for cell in row
                )
                peak_fire_front = max(peak_fire_front, burning_now)
                if burning_now == 0:
                    done = True
                    break

            last_processed += len(new_lines)

        kill_proc_tree(proc.pid)

        total_burned += max_percent_burned
        total_duration += frame_counter
        total_final_burned += percent_burned
        total_peak_front += peak_fire_front
        print(f" Done. Frames: {frame_counter}, Burned: {percent_burned:.1f}%")

    wind_strengths.append(wind_strength)
    max_burned_percents.append(total_burned / REPEATS)
    burn_durations.append(total_duration / REPEATS)
    final_burned_percents.append(total_final_burned / REPEATS)
    peak_fire_fronts.append(total_peak_front / REPEATS)

print("\nAll simulations finished!\n")

# === Plotting ===
fig, axs = plt.subplots(2, 2, figsize=(14, 10))
fig.suptitle("Forest Fire Simulation Metrics vs Wind Strength (Averaged)", fontsize=16)

axs[0, 0].plot(wind_strengths, max_burned_percents, marker="o")
axs[0, 0].set_title("Max Burned % of Burnable Cells")
axs[0, 0].set_xlabel("Wind Strength (km/h)")
axs[0, 0].set_ylabel("Max Burned (%)")
axs[0, 0].grid()

axs[0, 1].plot(wind_strengths, burn_durations, marker="o")
axs[0, 1].set_title("Burn Duration (frames)")
axs[0, 1].set_xlabel("Wind Strength (km/h)")
axs[0, 1].set_ylabel("Duration (frames)")
axs[0, 1].grid()

axs[1, 0].plot(wind_strengths, final_burned_percents, marker="o")
axs[1, 0].set_title("Final Burned % of Burnable Cells (at end)")
axs[1, 0].set_xlabel("Wind Strength (km/h)")
axs[1, 0].set_ylabel("Final Burned (%)")
axs[1, 0].grid()

axs[1, 1].plot(wind_strengths, peak_fire_fronts, marker="o")
axs[1, 1].set_title("Peak Fire Front (cells)")
axs[1, 1].set_xlabel("Wind Strength (km/h)")
axs[1, 1].set_ylabel("Peak Burning (cells)")
axs[1, 1].grid()

plt.tight_layout(rect=[0, 0.03, 1, 0.95])
plt.savefig("../res/fire_metrics_vs_wind_strength_averaged.png", dpi=150)
print("Plot saved to res/fire_metrics_vs_wind_strength_averaged.png")
