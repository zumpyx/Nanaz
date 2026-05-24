from base64 import b64encode, b64decode
from cryptography.hazmat.primitives.asymmetric import x25519

base64_encode_safe = lambda data: b64encode(data).rstrip(b'=').decode('utf-8').replace('+', '-1').replace('/', '-2')
base64_decode_safe = lambda data: b64decode((data := data.replace('-1', '+').replace('-2', '/')) + '=' * (-len(data) % 4))

def generate_noise_keypair():
    private_key = x25519.X25519PrivateKey.generate()
    public_key = private_key.public_key()
    private_bytes = private_key.private_bytes_raw()
    public_bytes = public_key.public_bytes_raw()
    return private_bytes, public_bytes

# 生成两套密钥
server_private_key, server_public_key = generate_noise_keypair()
client_private_key, client_public_key = generate_noise_keypair()

server_priv_b64 = base64_encode_safe(server_private_key)
server_pub_b64 = base64_encode_safe(server_public_key)
client_priv_b64 = base64_encode_safe(client_private_key)
client_pub_b64 = base64_encode_safe(client_public_key)


print(f'let server_private_key_b64 = "{server_priv_b64}";')
print(f'let server_public_key_b64 = "{server_pub_b64}";')
print(f'let client_private_key_b64 = "{client_priv_b64}";')
print(f'let client_public_key_b64 = "{client_pub_b64}";')

server_private_key = base64_decode_safe(server_priv_b64)
server_public_key = base64_decode_safe(server_pub_b64)
client_private_key = base64_decode_safe(client_priv_b64)
client_public_key = base64_decode_safe(client_pub_b64)

print(f"server_private_key: {server_private_key}")
print(f"server_public_key: {server_public_key}")
print(f"client_private_key: {client_private_key}")
print(f"client_public_key: {client_public_key}")
