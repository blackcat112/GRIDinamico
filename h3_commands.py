import h3

cell_id = "89390ca36cbffff"

centroid = h3.cell_to_latlng(cell_id) # Devuelve las coordenadas del centroide del hexágono
print("Centroide:", centroid)

boundary = h3.cell_to_boundary(cell_id) # Devuelve las coordenadas de el hexagono
print("Boundaries (polygon):", boundary)

res = h3.get_resolution(cell_id) # Obtener la resolución del cell_id
print("Resolución:", res)

parent = h3.cell_to_parent(cell_id, res - 1) # Obtener el padre en la resolución anterior
print("Padre (res-1):", parent)

