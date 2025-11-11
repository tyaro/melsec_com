use std::env;

use melsec_mc::endpoint::ConnectionTarget;
use melsec_mc::init_defaults;
use melsec_mc::mc_client::McClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_defaults()?;

    let addr = env::var("PLC_ADDR").unwrap_or_else(|_| "192.168.1.40".into());
    let port: u16 = env::var("PLC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4020);

    let target = ConnectionTarget::direct(addr.clone(), port);
    let client = McClient::new().with_target(target).with_monitoring_timer(5);

    println!(
        "Writing various typed values starting at D1010 on {}:{}",
        addr, port
    );

    // u16 block at D1010
    let u16_vals: Vec<u16> = (0u16..10u16).collect();
    let res = client.write_words_as("D1010", &u16_vals).await?;
    println!("wrote u16 -> response: {:?}", res);
    // read back u16 and verify
    let read_back: Vec<u16> = client
        .read_words_as::<u16>("D1010", u16_vals.len() as u16)
        .await?;
    println!("read back u16: {:?}", read_back);
    if read_back != u16_vals {
        return Err(format!(
            "verification failed for D1010 u16: wrote={:?} read={:?}",
            u16_vals, read_back
        )
        .into());
    }

    // i16 block at D1020
    let i16_vals: Vec<i16> = (0i16..10i16).collect();
    let res = client.write_words_as("D1020", &i16_vals).await?;
    println!("wrote i16 -> response: {:?}", res);
    // read back i16 and verify
    let read_back_i16: Vec<i16> = client
        .read_words_as::<i16>("D1020", i16_vals.len() as u16)
        .await?;
    println!("read back i16: {:?}", read_back_i16);
    if read_back_i16 != i16_vals {
        return Err(format!(
            "verification failed for D1020 i16: wrote={:?} read={:?}",
            i16_vals, read_back_i16
        )
        .into());
    }

    // u32 block at D1030 (each u32 uses 2 words)
    let u32_vals: Vec<u32> = vec![0x11223344u32, 0x55667788u32, 0x99AABBCCu32];
    let res = client.write_words_as("D1030", &u32_vals).await?;
    println!("wrote u32 -> response: {:?}", res);
    // read back u32 and verify
    let read_back_u32: Vec<u32> = client
        .read_words_as::<u32>("D1030", u32_vals.len() as u16)
        .await?;
    println!("read back u32: {:?}", read_back_u32);
    if read_back_u32 != u32_vals {
        return Err(format!(
            "verification failed for D1030 u32: wrote={:?} read={:?}",
            u32_vals, read_back_u32
        )
        .into());
    }

    // i32 block at D1040
    let i32_vals: Vec<i32> = vec![-1i32, 0, 12345678];
    let res = client.write_words_as("D1040", &i32_vals).await?;
    println!("wrote i32 -> response: {:?}", res);
    // read back i32 and verify
    let read_back_i32: Vec<i32> = client
        .read_words_as::<i32>("D1040", i32_vals.len() as u16)
        .await?;
    println!("read back i32: {:?}", read_back_i32);
    if read_back_i32 != i32_vals {
        return Err(format!(
            "verification failed for D1040 i32: wrote={:?} read={:?}",
            i32_vals, read_back_i32
        )
        .into());
    }

    // f32 block at D1050
    let f32_vals: Vec<f32> = vec![1.2345f32, -2.5f32, std::f32::consts::PI];
    let res = client.write_words_as("D1050", &f32_vals).await?;
    println!("wrote f32 -> response: {:?}", res);
    // read back f32 and verify with small tolerance
    let read_back_f32: Vec<f32> = client
        .read_words_as::<f32>("D1050", f32_vals.len() as u16)
        .await?;
    println!("read back f32: {:?}", read_back_f32);
    let tol = 1e-5f32;
    if read_back_f32.len() != f32_vals.len() {
        return Err(format!(
            "verification failed for D1050 f32: length mismatch wrote={} read={}",
            f32_vals.len(),
            read_back_f32.len()
        )
        .into());
    }
    for (i, (&w, &r)) in f32_vals.iter().zip(read_back_f32.iter()).enumerate() {
        if (w - r).abs() > tol {
            return Err(format!(
                "verification failed for D1050 f32 at index {}: wrote={} read={} diff={}",
                i,
                w,
                r,
                (w - r).abs()
            )
            .into());
        }
    }

    // bits block at D1060 writing [bool;16]
    let bits: [bool; 16] = [
        true, false, true, false, true, false, true, false, true, false, true, false, true, false,
        true, false,
    ];
    let blocks: Vec<[bool; 16]> = vec![bits; 3];
    let res = client.write_words_as("D1060", &blocks).await?;
    println!("wrote bits -> response: {:?}", res);
    // read back bits and verify
    let read_back_bits: Vec<[bool; 16]> = client
        .read_words_as::<[bool; 16]>("D1060", blocks.len() as u16)
        .await?;
    println!("read back bits: {:?}", read_back_bits);
    if read_back_bits.len() != blocks.len() {
        return Err(format!(
            "verification failed for D1060 bits: length mismatch wrote={} read={}",
            blocks.len(),
            read_back_bits.len()
        )
        .into());
    }
    for (i, (w, r)) in blocks.iter().zip(read_back_bits.iter()).enumerate() {
        if w != r {
            return Err(format!(
                "verification failed for D1060 bits at index {}: wrote={:?} read={:?}",
                i, w, r
            )
            .into());
        }
    }

    println!("Done writing various types to D1010..D1060");
    Ok(())
}
