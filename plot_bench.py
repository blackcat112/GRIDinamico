import pandas as pd
import matplotlib.pyplot as plt

# Cargar CSV
df = pd.read_csv("bench_out_20250826_153336/summary.txt")

# Extraer resolución del nombre de archivo
df["resolution"] = df["File"].str.extract(r"res(\d+)").astype(int)
df["refine"] = df["File"].str.contains("refine")

# Ordenar
df = df.sort_values(["resolution", "refine"])

# Gráfico
plt.figure(figsize=(8,5))
for refine, group in df.groupby("refine"):
    label = "Refine ON" if refine else "Refine OFF"
    plt.plot(group["resolution"], group["Requests/sec"], marker="o", label=label)

plt.xlabel("Resolución H3")
plt.ylabel("Requests/sec")
plt.title("Benchmark H3 (Requests/sec vs Resolución)")
plt.legend()
plt.grid(True)
plt.savefig("bench_comparison.png")
plt.show()
