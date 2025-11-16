import os

def generate_random_file(filename, size_gb):
    """
    Generates a file filled with cryptographically secure random data.

    Args:
        filename (str): The name of the file to create.
        size_gb (float): The desired size of the file in gigabytes.
    """
    chunk_size = 1024 * 1024  # 1 MB
    target_bytes = int(size_gb * (1024 ** 3)) # Convert GB to bytes (Gibibytes)

    with open(filename, 'wb') as f:
        bytes_written = 0
        while bytes_written < target_bytes:
            # Determine how many bytes to write in this chunk
            bytes_to_write = min(chunk_size, target_bytes - bytes_written)
            
            # Generate random bytes
            random_data = os.urandom(bytes_to_write)
            
            # Write to file
            f.write(random_data)
            bytes_written += bytes_to_write
    print(f"File '{filename}' of {size_gb}GB has been created.")

# Example usage:
generate_random_file("random_1gb_file.bin", 1) 