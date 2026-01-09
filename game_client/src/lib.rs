use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Write;
use std::net::TcpStream;

//~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[no_mangle]
#[allow(clippy::too_many_arguments)]
fn game_client_init(
    argc: std::ffi::c_int,
    argv: *const *const std::ffi::c_char,
    inout_width: &mut std::ffi::c_int,
    inout_height: &mut std::ffi::c_int,
    inout_dt: &mut std::ffi::c_double,
) -> *mut std::ffi::c_void /* application */ {
    let args_utf8 = Vec::from_iter((0..argc).map(|a| {
        let c_ptr = unsafe { argv.offset(a as isize) };
        let c_str = unsafe { std::ffi::CStr::from_ptr(*c_ptr) };
        c_str.to_string_lossy()
    }));
    let args = Vec::from_iter(args_utf8.iter().map(|a| a.as_ref()));
    let mut w = *inout_width as usize;
    let mut h = *inout_height as usize;
    let mut dt = *inout_dt;

    match init_application(&args, &mut w, &mut h, &mut dt) {
        Ok(app) => {
            *inout_width = w as std::ffi::c_int;
            *inout_height = h as std::ffi::c_int;
            *inout_dt = dt as std::ffi::c_double;
            Box::into_raw(Box::new(app)) as *mut _
        }
        Err(e) => {
            eprintln!("ERROR: {}", e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
fn game_client_update(
    c_evt: *const std::ffi::c_char,
    x: std::ffi::c_int,
    y: std::ffi::c_int,
    w: std::ffi::c_int,
    h: std::ffi::c_int,
    btn: std::ffi::c_int,
    c_key: *const std::ffi::c_char,
    c_screen: *mut std::ffi::c_char,
    c_app: *mut std::ffi::c_void,
) -> std::ffi::c_int /* -1: quit    0: go-on    1: redraw */ {
    let evt = unsafe { std::ffi::CStr::from_ptr(c_evt) }.to_string_lossy();
    let key = unsafe { std::ffi::CStr::from_ptr(c_key) }.to_string_lossy();
    let point = Point { x, y };
    let mut screen = Screen {
        width: w as usize,
        height: h as usize,
        pixels: unsafe {
            std::slice::from_raw_parts_mut(
                c_screen as *mut Color,
                (w * h) as usize,
            )
        },
    };
    let app = unsafe { &mut *(c_app as *mut Application) };
    let status = update_application(
        evt.as_ref(),
        key.as_ref(),
        btn as usize,
        &point,
        &mut screen,
        app,
    )
    .unwrap_or_else(|e| {
        eprintln!("ERROR: {}", e);
        UpdateStatus::Quit
    });
    match status {
        UpdateStatus::GoOn => 0,
        UpdateStatus::Redraw => 1,
        UpdateStatus::Quit => {
            // ensure deallocation
            let _owned = unsafe { Box::from_raw(app) };
            -1
        }
    }
}

//~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
#[derive(Debug)]
struct ClientInfo {
    position: Point,
    image: Image,
}

#[derive(Debug)]
struct Screen<'a> {
    width: usize,
    height: usize,
    pixels: &'a mut [Color],
}

#[derive(Debug, Clone, Copy)]
enum UpdateStatus {
    GoOn,
    Redraw,
    Quit,
}

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

#[derive(Debug)]
struct Application {
    status: UpdateStatus,
    output: Option<TcpStream>,
    input: Option<TcpStream>,
    clients: HashMap<u32, ClientInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Image {
    width: usize,
    height: usize,
    pixels: Vec<Color>,
}
fn init_application(
    args: &[&str],
    width: &mut usize,
    height: &mut usize,
    dt: &mut f64,
) -> Result<Application, Box<dyn std::error::Error>> {
    println!("args: {:?}", args);
    *width = 800;
    *height = 600;
    *dt = 1.0 / 30.0;

    // Ajoute le chemin de l'image à la structure
    let image_path = if let Some(image_path) = args.get(2) {
        image_path.to_string()
    } else {
        return Err(
            "Aucun chemin vers une image n'est fourni dans l'argument".into(),
        );
    };

    // Initialisation de la connexion au serveur
    let (mut output, input) = init_server(args)?;

    let clients = std::collections::HashMap::new();
    if let Ok(image) = load_image(&image_path) {
        println!("Image chargée avec succès.");

        match serde_json::to_string(&image) {
            Ok(json_image) => {
                let msg = format!("image {}\n", json_image);
                output.write_all(msg.as_bytes())?;
                output.flush()?;
                println!("Image envoyée au serveur.");
            }
            Err(e) => {
                eprintln!("Erreur de sérialisation de l'image : {}", e);
            }
        }
    } else {
        eprintln!(
            "Échec du chargement de l'image à partir de {}",
            image_path
        );
    }

    println!("{}×{}@{:.3}", width, height, dt);

    Ok(Application {
        status: UpdateStatus::GoOn,
        output: Some(output),
        input: Some(input),
        clients,
    })
}

fn update_application(
    evt: &str,
    key: &str,
    btn: usize,
    point: &Point,
    screen: &mut Screen,
    app: &mut Application,
) -> Result<UpdateStatus, Box<dyn std::error::Error>> {
    let _maybe_unused = /* prevent some warnings */ (btn, point);
    if evt != "T" {
        println!(
            "evt={:?} btn={} key={:?} ({};{}) {}×{}",
            evt, btn, key, point.x, point.y, screen.width, screen.height
        );
    }

    app.status = UpdateStatus::GoOn;

    // Remplacer la couleur de fond
    for c in screen.pixels.iter_mut() {
        c.r = 0;
        c.g = 0;
        c.b = 0;
    }

    // Gérer l'événement et envoyer au serveur
    if let Some(motion) = handle_event(app, evt, key) {
        println!("motion: {:?}", motion);
        if let Some(output) = app.output.as_mut() {
            // Sérialiser motion
            match serde_json::to_string(&motion) {
                Ok(json_motion) => {
                    println!("json_serialized_motion: {:?}", json_motion);
                    let msg = format!("motion {}\n", json_motion);
                    output.write_all(msg.as_bytes())?;
                    output.flush()?;
                    app.status = UpdateStatus::Redraw;
                }
                Err(e) => {
                    eprintln!("Erreur lors de la sérialisation JSON du mouvement : {}", e);
                }
            }
        } else {
            println!("Aucun flux de sortie disponible pour envoyer la demande au serveur.");
        }
    }

    handle_messages(app)?;
    redraw_if_needed(app, screen);

    Ok(app.status)
}

fn handle_event(
    app: &mut Application,
    evt: &str,
    key: &str,
) -> Option<Point> {
    let mut motion = None;
    match evt {
        "C" => app.status = UpdateStatus::Redraw,
        "Q" => app.status = UpdateStatus::Quit,
        "KP" => match key {
            "Escape" => app.status = UpdateStatus::Quit,
            "Left" => motion = Some(Point { x: -10, y: 0 }),
            "Right" => motion = Some(Point { x: 10, y: 0 }),
            "Up" => motion = Some(Point { x: 0, y: -10 }),
            "Down" => motion = Some(Point { x: 0, y: 10 }),
            " " => app.status = UpdateStatus::Redraw,
            _ => {}
        },
        _ => {}
    }
    motion
}

fn redraw_if_needed(
    app: &Application,
    screen: &mut Screen,
) {
    if let UpdateStatus::Redraw = app.status {
        // Effacer l'écran en appliquant une transformation de couleur (exemple arbitraire)
        for c in screen.pixels.iter_mut() {
            let (r, g, b) =
                (c.r as u32 + 10, c.g as u32 + 25, c.b as u32 + 35);
            c.r = r as u8;
            c.g = g as u8;
            c.b = b as u8;
        }

        // Dessiner l'image principale à la position actuelle
        let transparent_color = Some(Color { r: 0, g: 255, b: 0 });

        // Dessiner les images des autres clients
        for (id, client) in &app.clients {
            println!(
                "Dessiner le client {} à la position {:?}",
                id, client.position
            );

            draw_image(
                screen,
                &client.image,
                client.position,
                transparent_color,
            );
        }
    }
}

//~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn load_image(path: &str) -> Result<Image, Box<dyn std::error::Error>> {
    use std::fs;
    println!("Chargement de l'image à partir de : {}", path);

    if !std::path::Path::new(path).exists() {
        return Err(format!("Le fichier '{}' est introuvable.", path).into());
    }
    let content = fs::read(path)?;

    // Construire un itérateur
    let mut words = std::str::from_utf8(&content)?
        .lines()
        .map(|l| l.find('#').map_or(l, |pos| &l[0..pos]))
        .flat_map(|l| l.split_whitespace())
        .filter(|w| !w.is_empty());

    // Vérifier le marqueur de format
    match words.next() {
        Some("P3") => (),
        _ => return Err("Invalid format marker (expected P3)".into()),
    }

    // Extraire la largeur et la hauteur
    let width = words.next().ok_or("Missing width")?.parse::<usize>()?;
    let height = words.next().ok_or("Missing height")?.parse::<usize>()?;

    // Vérifier la valeur maximale de la couleur (doit être 255)
    match words.next() {
        Some("255") => (),
        _ => return Err("Invalid max value (expected 255)".into()),
    }

    // Charger les pixels
    let mut pixels = Vec::with_capacity(width * height);
    while let (Some(r), Some(g), Some(b)) =
        (words.next(), words.next(), words.next())
    {
        let color = Color {
            r: r.parse::<u8>()?,
            g: g.parse::<u8>()?,
            b: b.parse::<u8>()?,
        };
        pixels.push(color);
    }

    // Vérifier que tous les pixels ont été chargés
    if pixels.len() != width * height {
        return Err("Pixel count does not match width × height".into());
    }

    Ok(Image {
        width,
        height,
        pixels,
    })
}

fn draw_image(
    screen: &mut Screen,
    image: &Image,
    position: Point,
    transparent_color: Option<Color>,
) {
    let p0 = Point {
        x: position.x.clamp(0, screen.width as i32),
        y: position.y.clamp(0, screen.height as i32),
    };
    let p1 = Point {
        x: (position.x + image.width as i32).clamp(0, screen.width as i32),
        y: (position.y + image.height as i32).clamp(0, screen.height as i32),
    };
    let dx = 0.max(p0.x - position.x);
    let dy = 0.max(p0.y - position.y);
    let mut image_idx = dy as usize * image.width + dx as usize;
    let mut screen_idx = p0.y as usize * screen.width + p0.x as usize;
    let w = 0.max(p1.x - p0.x) as usize;
    for _ in p0.y..p1.y {
        let src = &image.pixels[image_idx..image_idx + w];
        let dst = &mut screen.pixels[screen_idx..screen_idx + w];
        match transparent_color {
            None => {
                dst.copy_from_slice(src);
            }
            Some(tr) => {
                for (src_pixel, dst_pixel) in src.iter().zip(dst.iter_mut()) {
                    if *src_pixel != tr {
                        *dst_pixel = *src_pixel;
                    }
                }
            }
        }

        image_idx += image.width;
        screen_idx += screen.width;
    }
}

fn init_server(
    args: &[&str]
) -> Result<(TcpStream, TcpStream), Box<dyn std::error::Error>> {
    // Récupération des arguments
    let server_name =
        args.get(3).ok_or("Server name not provided in arguments")?;
    let port = args.get(4).ok_or("Port not provided in arguments")?;
    let server_address = format!("{}:{}", server_name, port);

    // Connexion au serveur
    println!("Connecting to server at {}", server_address);
    let stream = TcpStream::connect(&server_address).map_err(|e| {
        eprintln!("Failed to connect to server: {}", e);
        e
    })?;
    println!("Connected to server at {}", server_address);

    let output = stream.try_clone()?;
    let input = stream;

    Ok((output, input))
}

fn read_lines_nonblocking(
    input: &mut BufReader<TcpStream>
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    fn inner(
        input: &mut BufReader<TcpStream>
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            match input.read_line(&mut line) {
                Ok(r) => {
                    if !line.is_empty() {
                        lines.push(line);
                    }
                    if r == 0 {
                        lines.push(String::new());
                        break;
                    }
                }
                Err(e) => {
                    if e.kind() != ErrorKind::WouldBlock {
                        Err(e)?
                    }
                    if line.is_empty() {
                        break;
                    }
                }
            }
        }
        Ok(lines)
    }
    input.get_mut().set_nonblocking(true)?;
    let result = inner(input);
    input.get_mut().set_nonblocking(false)?;
    result
}

fn handle_messages(
    app: &mut Application
) -> Result<(), Box<dyn std::error::Error>> {
    match app.input.take() {
        Some(stream) => {
            // Créer un BufReader à partir du TcpStream
            let mut reader = std::io::BufReader::new(stream.try_clone()?);

            let lines = read_lines_nonblocking(&mut reader).map_err(|e| {
                format!("Erreur de lecture des lignes : {}", e)
            })?;

            for line in lines {
                if line.trim().is_empty() {
                    app.status = UpdateStatus::Quit;
                    return Ok(());
                }

                if let Some(data) = line.strip_prefix("image ") {
                    match serde_json::from_str::<(u32, Image, Point)>(
                        data.trim(),
                    ) {
                        Ok((id, image, position)) => {
                            app.clients
                                .insert(id, ClientInfo { position, image });
                            println!("Nouveau client ajouté : id={}, position={:?}", id, position);
                            app.status = UpdateStatus::Redraw;
                        }
                        Err(e) => {
                            eprintln!("Erreur de désérialisation JSON pour image : {}", e);
                        }
                    }
                } else if let Some(data) = line.strip_prefix("client_left ") {
                    match data.trim().parse::<u32>() {
                        Ok(id) => {
                            if app.clients.remove(&id).is_some() {
                                println!("Client supprimé : id={}", id);
                                app.status = UpdateStatus::Redraw;
                            } else {
                                println!(
                                    "Client inconnu à supprimer : id={}",
                                    id
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Erreur de parsing de l'identifiant client_left : {}", e);
                        }
                    }
                } else if let Some(data) = line.strip_prefix("position ") {
                    match serde_json::from_str::<(u32, Point)>(data) {
                        Ok((id, new_position)) => {
                            if let Some(client) = app.clients.get_mut(&id) {
                                client.position = new_position;
                                println!(
                                    "Position mise à jour pour le client id={}: {:?}",
                                    id, new_position
                                );
                                app.status = UpdateStatus::Redraw;
                            } else {
                                println!("Client inconnu pour mise à jour de position : id={}", id);
                            }
                        }
                        Err(e) => {
                            eprintln!("Erreur de désérialisation JSON pour position_update : {}", e);
                        }
                    }
                }
                if let Some(data) = line.strip_prefix("all_clients") {
                    match serde_json::from_str::<Vec<(u32, Image, Point)>>(
                        data,
                    ) {
                        Ok(client_list) => {
                            for (id, image, position) in client_list {
                                app.clients.insert(
                                    id,
                                    ClientInfo { position, image },
                                );
                                println!(
                                    "Nouveau client ajouté : id={}, position={:?}",
                                    id, position
                                );
                            }
                            app.status = UpdateStatus::Redraw;
                        }
                        Err(e) => {
                            eprintln!(
                                "Erreur de désérialisation JSON pour all_clients : {}",
                                e
                            );
                        }
                    }
                } else {
                    println!("Message reçu : {}", line);
                }
            }

            app.input = Some(stream);
        }
        None => {
            return Err("Aucun TcpStream valide dans app.input".into());
        }
    }

    Ok(())
}
