# TCP-Client-Serveur
Programme Rust client/serveur qui permet à chaque client connecté au serveur de déplacer en temps réel un personnage initié par le serveur et et visualisé par les autres clients

## Installation
Pour récupérer le dépot : 

``git clone https://github.com/ewannezou/TCP-Client-Serveur``

## Utilisation
Pour lancez le server, éxécuter les commandes : 

``cd game_server``

 `` cargo run ``
  
Pour lancer les clients : ouvrer 2 terminals différents, puis éxécuter les commandes : 

``cd game_client``

Sur Linux :

``./run_client.sh data/cat01.ppm localhost 5555``

Sur Windows : 

``./run_client.bat data/cat01.ppm localhost 5555``

Deux chats apparaissent et bougent simultanément selon les déplacements que vous choisissez
