// MCProtocolをインポート
use mcp3e::mc_protocol::QMCProtocol;


// メイン関数(TOKIO)
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>> {
    let ip = (192,168,8,11);
    let port = 1025;
    let config = mcp3e::mc_protocol::Config {
        network_no:0x00,
        pc_no:0xff,
        request_unit_io_no:0x03ff,
        request_unit_station_no:0x00,
        monitor_timer:0x0002,
    };

    //　MCProtocolのインスタンスを生成
    let mut mc_protocol = QMCProtocol::new(ip,port,config);
    //　接続処理を実行
    mc_protocol.connect().await?;


    // ワード一括読込
    let result = mc_protocol.block_read_word("W20", 20).await;
    println!("{:?}",result);
    
    // ワード一括読込Rawデータ
    let result = mc_protocol.block_read_word_raw("W20", 20).await;
    println!("{:04x?}",result);

    // ビット一括読込
    let result = mc_protocol.block_read_bit("M06", 7).await;
    println!("{:?}",result);
    
    // ワード一括書込
    let write_data:Vec<i16> = vec![32767,-32768,32235,432];
    let result = mc_protocol.block_write_word("D1000",&write_data).await;
    println!("{:?}",result);
    
    // ビット一括書込
    let write_data:Vec<u8> = vec![1,1,0,0,1,1,0,0,0,1];
    let result = mc_protocol.block_write_bit("M010",&write_data).await;
    println!("{:?}",result);

    Ok(())
    
    
}
