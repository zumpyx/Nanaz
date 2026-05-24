use data_encoding::BASE64_NOPAD;
use std::{error::Error, str::FromStr};

pub fn base64_encode_safe(data: &[u8]) -> Result<String, data_encoding::DecodeError> {
    Ok(BASE64_NOPAD
        .encode(data)
        .replace("+", "-1")
        .replace("/", "-2"))
}

pub fn base64_decode_safe(data: &str) -> Result<Vec<u8>, data_encoding::DecodeError> {
    let data = data.replace("-1", "+").replace("-2", "/");
    let decoded = BASE64_NOPAD.decode(data.as_bytes())?;
    Ok(decoded)
}

fn print_b64(label: &str, data: &[u8]) {
    println!("[b64] {}: {}", label, base64_encode_safe(data).unwrap(),);
}

fn main() -> Result<(), Box<dyn Error>> {
    let pattern = snow::params::NoiseParams::from_str("Noise_KK_25519_ChaChaPoly_BLAKE2s")?;

    let server_private_key_b64 = "IEROc0alobAJlH8t-2Onhqy01roEv3p3SMi5v4QVWR08";
    let server_public_key_b64 = "uZTlbt5vQanO6JgbfO2TQkISHSUOlrIynMFE4bXN2wQ";
    let client_private_key_b64 = "6L8Wqp8P4JguiPT2mArDpfEzbQfaWc-1HF0zr9cDz6XU";
    let client_public_key_b64 = "IZ-2pT6tzOmwhv0yPRJrlcd9Enh-2Nzv6g42xAD5WumCw";

    let server_public_key = base64_decode_safe(&server_public_key_b64)?;
    let server_private_key = base64_decode_safe(&server_private_key_b64)?;
    let client_private_key = base64_decode_safe(&client_private_key_b64)?;
    let client_public_key = base64_decode_safe(&client_public_key_b64)?;

    println!("pattern: {:#?}", &pattern);

    let mut client_handshake = snow::Builder::new(pattern.clone())
        .local_private_key(&client_private_key)?
        .remote_public_key(&server_public_key)?
        .build_responder()?;

    let mut server_handshake = snow::Builder::new(pattern)
        .local_private_key(&server_private_key)?
        .remote_public_key(&client_public_key)?
        .build_initiator()?;

    let mut msg1 = [0u8; 1024];
    let mut msg2 = [0u8; 1024];
    let mut read_buf = [0u8; 1024];

    // =========================
    // 1. server -> implant
    // =========================

    println!("========== handshake 1: server -> implant ==========");
    let msg1_len = server_handshake.write_message(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], &mut msg1)?;
    let msg1 = &msg1[..msg1_len];

    print_b64("server sends handshake message 1", msg1);

    let payload1_len = client_handshake.read_message(msg1, &mut read_buf)?;
    let payload1 = &read_buf[..payload1_len];

    print_b64("implant decrypted handshake payload 1", payload1);

    // =========================
    // 2. implant -> server
    // =========================

    println!();
    println!("========== handshake 2: implant -> server ==========");
    let msg2_len = client_handshake.write_message(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9], &mut msg2)?;
    let msg2 = &msg2[..msg2_len];

    print_b64("implant sends handshake message 2", msg2);

    let payload2_len = server_handshake.read_message(msg2, &mut read_buf)?;
    let payload2 = &read_buf[..payload2_len];

    print_b64("server decrypted handshake payload 2", payload2);

    // =========================
    // 3. 检查握手 hash
    // =========================

    println!();
    println!("========== handshake hash ==========");
    let implant_hash = client_handshake.get_handshake_hash();
    let server_hash = server_handshake.get_handshake_hash();

    print_b64("implant handshake hash", implant_hash);
    print_b64("server handshake hash", server_hash);

    println!("handshake hash match = {}", implant_hash == server_hash);

    // =========================
    // 4. 进入 transport mode
    // =========================

    let mut implant_transport = client_handshake.into_transport_mode()?;
    let mut server_transport = server_handshake.into_transport_mode()?;

    // =========================
    // 5. 测试加密通信
    // =========================

    println!();
    println!("========== transport test ==========");

    let plaintext = b"asd";

    let mut encrypted = [0u8; 1024 * 64];
    let mut decrypted = [0u8; 1024 * 64];

    let encrypted_len = server_transport.write_message(plaintext, &mut encrypted)?;
    let encrypted_msg = &encrypted[..encrypted_len];

    print_b64("server encrypted message", encrypted_msg);

    let decrypted_len = implant_transport.read_message(encrypted_msg, &mut decrypted)?;
    let decrypted_msg = &decrypted[..decrypted_len];

    println!(
        "implant decrypted message = {}",
        String::from_utf8_lossy(decrypted_msg)
    );

    Ok(())
}
