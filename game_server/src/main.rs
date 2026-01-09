use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Image {
    width: usize,
    height: usize,
    pixels: Vec<Color>,
}

#[derive(Debug)]
struct ClientInfo {
    position: Point,
    image: Image,
    stream: TcpStream,
}
#[derive(Debug)]
struct ServerState {
    next_id: u32, // Pour générer des identifiants uniques
    clients: HashMap<u32, ClientInfo>, // Associe chaque ID à son ClientInfo
    positions: HashMap<u32, Point>, // Positions de tous les clients
    images: HashMap<u32, Vec<u8>>, // Images des clients (format binaire)
}

type SharedServerState = Arc<Mutex<ServerState>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tcp_port = 5555;
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, tcp_port))?;
    println!(
        "Serveur TCP en attente de connexions sur le port {}",
        tcp_port
    );

    // État partagé entre tous les threads
    let state: SharedServerState = Arc::new(Mutex::new(ServerState {
        next_id: 1,
        clients: HashMap::new(),
        positions: HashMap::new(),
        images: HashMap::new(),
    }));

    // Boucle principale du serveur
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let state_clone = Arc::clone(&state);
                std::thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, state_clone) {
                        eprintln!("Erreur : {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Erreur de connexion entrante : {}", e),
        }
    }

    Ok(())
}

fn handle_connection(
    stream: TcpStream,
    state: SharedServerState,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = stream.try_clone()?;
    let mut input = BufReader::new(stream);

    // Enregistrement initial : génération d'un identifiant unique
    let client_id;
    {
        let mut state = state.lock().unwrap();

        client_id = state.next_id;
        state.next_id += 1;

        // Ajouter le client à l'état global avec des champs vides pour l'image et la position
        state.clients.insert(
            client_id,
            ClientInfo {
                position: Point { x: 0, y: 0 },
                image: Image {
                    width: 0,
                    height: 0,
                    pixels: Vec::new(),
                },
                stream: output.try_clone()?,
            },
        );
    }

    println!("Client {} connecté, en attente de données.", client_id);

    // Boucle principale : surveiller les messages du client
    loop {
        let mut request = String::new();
        let r = input.read_line(&mut request)?;
        if r == 0 {
            // Fin de communication
            handle_disconnect(client_id, &state)?;
            break;
        }

        if let Some(input) = request.strip_prefix("motion ") {
            // Demande de déplacement
            match serde_json::from_str::<Point>(input.trim()) {
                Ok(delta) => {
                    let new_position =
                        handle_motion(client_id, delta, &state)?;
                    println!(
                        "Client {} déplacé vers la nouvelle position {:?}",
                        client_id, new_position
                    );
                }
                Err(e) => {
                    eprintln!(
                        "Message de déplacement mal formaté {} : {}",
                        input, e
                    );
                }
            }
        }
        if let Some(input) = request.strip_prefix("image ") {
            // Récupération de l'image
            match serde_json::from_str::<Image>(input.trim()) {
                Ok(image) => {
                    handle_image(client_id, image, &state)?;
                    println!("Client {} registered", client_id);
                }
                Err(e) => {
                    eprintln!(
                        "Erreur de désérialisation JSON dans '{}': {}",
                        input, e
                    );
                }
            }
        } else {
            println!(
                "Message reçu du client {} : {}",
                client_id,
                request.trim()
            );
        }
    }

    Ok(())
}

fn handle_motion(
    client_id: u32,
    delta: Point,
    state: &SharedServerState,
) -> Result<Point, Box<dyn std::error::Error>> {
    let mut state = state.lock().unwrap();

    if let Some(client) = state.clients.get_mut(&client_id) {
        client.position.x += delta.x;
        client.position.y += delta.y;

        // Limiter la position aux dimensions autorisées (par exemple, 800x600)
        client.position.x = client.position.x.clamp(0, 800);
        client.position.y = client.position.y.clamp(0, 600);

        // Nouvelle position à envoyer
        let new_position = client.position;

        // Envoyer la nouvelle position au client
        let reply = serde_json::to_string(&(client_id, new_position))?;
        client
            .stream
            .write_all(format!("position {}\n", reply).as_bytes())?;
        client.stream.flush()?;

        // Envoyer la nouvelle position à tous les autres clients
        let position_update =
            serde_json::to_string(&(client_id, new_position))?;

        for (&other_id, other_client) in &mut state.clients {
            if other_id != client_id {
                if let Err(e) = other_client.stream.write_all(
                    format!("position {}\n", position_update).as_bytes(),
                ) {
                    eprintln!(
                        "Erreur lors de l'envoi de la mise à jour au client {} : {}",
                        other_id, e
                    );
                }
            }
        }

        // Retourner la nouvelle position
        Ok(new_position)
    } else {
        // Retourner une erreur si le client n'existe pas
        Err(format!("Client {} introuvable", client_id).into())
    }
}

fn handle_image(
    client_id: u32,
    image: Image,
    state: &SharedServerState,
) -> Result<(u32, Point), Box<dyn std::error::Error>> {
    // Verrouiller l'état partagé
    let mut state = state.lock().unwrap();

    // Vérifier si le client existe dans l'état
    let position = if let Some(client) = state.clients.get_mut(&client_id) {
        let mut rng = rand::thread_rng();
        let new_position = Point {
            x: rng.gen_range(0..800),
            y: rng.gen_range(0..600),
        };

        // Mettre à jour la position et l'image du client dans l'état
        client.position = new_position;
        client.image = image.clone();

        // Envoyer la paire (id, position) au client
        let reply =
            serde_json::to_string(&(client_id, image.clone(), new_position))?;
        client
            .stream
            .write_all(format!("image {}\n", reply).as_bytes())?;
        client.stream.flush()?;

        new_position
    } else {
        // Retourner une erreur si le client n'existe pas
        return Err(format!("Client {} introuvable", client_id).into());
    };

    // Créer la liste des clients avec leur id, image, et position à envoyer au client courant
    let all_clients_data: Vec<(u32, Image, Point)> = state
        .clients
        .iter()
        .map(|(&id, client)| (id, client.image.clone(), client.position))
        .collect();

    let all_clients_message = serde_json::to_string(&all_clients_data)?;

    // Envoyer la liste des données des clients au client actuel et à tous les autres clients
    let clients_snapshot: Vec<_> = state.clients.iter().collect();

    for (&other_id, client) in clients_snapshot {
        if let Ok(mut stream) = client.stream.try_clone() {
            if let Err(e) = stream.write_all(
                format!("all_clients {}\n", all_clients_message).as_bytes(),
            ) {
                eprintln!(
                    "Erreur lors de l'envoi de la mise à jour au client {} : {}",
                    other_id, e
                );
            }
            if let Err(e) = stream.flush() {
                eprintln!(
                    "Erreur lors de la tentative de flush du flux pour le client {} : {}",
                    other_id, e
                );
            }
        } else {
            eprintln!(
                "Erreur lors de la tentative de clonage du flux pour le client {}",
                other_id
            );
        }
    }

    println!(
        "Client {} mis à jour avec une nouvelle image et position {:?}",
        client_id, position
    );

    // Retourner l'id du client et la position
    Ok((client_id, position))
}

fn handle_disconnect(
    client_id: u32,
    state: &SharedServerState,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = state.lock().unwrap();

    if state.clients.remove(&client_id).is_some() {
        state.positions.remove(&client_id);
        state.images.remove(&client_id);

        // Diffuser l'identifiant du client partant
        let message = format!("client_left {}\n", client_id);
        for (&other_id, other_client) in &mut state.clients {
            if let Err(e) = other_client.stream.write_all(message.as_bytes())
            {
                eprintln!("Erreur d'envoi au client {} : {}", other_id, e);
            }
        }

        println!("Client {} déconnecté et supprimé.", client_id);
    }

    Ok(())
}
