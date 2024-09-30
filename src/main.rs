use config::Config;
use paho_mqtt as mqtt;
use rppal::gpio::{Gpio, Trigger};
use serde::Deserialize;
use std::{error::Error, process, thread::sleep, time::Duration};

const QOS: i32 = 1;

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Settings {
    mqtt_url: String,
    mqtt_username: String,
    mqtt_password: String,
    mqtt_topic: String,
    mqtt_message: String,
    gpio_port: u8,
}

impl Settings {
    pub fn new() -> Result<Self, config::ConfigError> {
        let settings = Config::builder()
            .add_source(config::File::with_name("config").required(false))
            .add_source(config::File::with_name("/etc/doorbell/config").required(false))
            .add_source(config::Environment::with_prefix("APP"))
            .build()?;
        settings.try_deserialize()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let settings = Settings::new()?;

    let cli = connect_mqtt(&settings);
    let topic = mqtt::Topic::new(&cli, &settings.mqtt_topic, QOS);

    println!("Setting up GPIO using GPIO {}...", &settings.gpio_port);
    let mut pin = Gpio::new()?.get(settings.gpio_port)?.into_input_pullup();
    pin.set_interrupt(Trigger::RisingEdge, None)?;

    listen_to_doorbell(pin, topic, &settings);

    // Disconnect from the broker
    let tok = cli.disconnect(None);
    tok.wait().unwrap();
    Ok(())
}

fn listen_to_doorbell(
    mut pin: rppal::gpio::InputPin,
    topic: paho_mqtt::Topic,
    settings: &Settings,
) {
    loop {
        match pin.poll_interrupt(true, None) {
            Ok(event) => {
                if let Some(event) = event {
                    match event.trigger {
                        rppal::gpio::Trigger::RisingEdge => {
                            println!("{} Doorbell pressed", chrono::Local::now());
                            let tok = topic.publish(settings.mqtt_message.clone());
                            if let Err(e) = tok.wait() {
                                println!("Error sending message: {:?}", e);
                                break;
                            }
                        }
                        _ => println!("Disabled"),
                    }
                }
                sleep(Duration::from_secs(4));
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }
}

fn connect_mqtt(settings: &Settings) -> paho_mqtt::AsyncClient {
    let cli = mqtt::AsyncClient::new(settings.mqtt_url.clone()).unwrap_or_else(|err| {
        println!("Error creating the client: {}", err);
        process::exit(1);
    });

    let ssl_opts = mqtt::SslOptionsBuilder::new()
        .verify(false)
        .enable_server_cert_auth(false)
        .finalize();

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .ssl_options(ssl_opts)
        .user_name(settings.mqtt_username.clone())
        .password(settings.mqtt_password.clone())
        .finalize();

    // Connect and wait for it to complete or fail
    println!("Connecting to the MQTT broker at {}...", &settings.mqtt_url);
    if let Err(e) = cli.connect(conn_opts).wait() {
        println!("Unable to connect: {:?}", e);
        process::exit(1);
    }
    cli
}
