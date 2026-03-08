To run project

(as server)
from root, do ``cargo run --bin game_client``
then, click "Start Server"
the console will then say something like
```
starting server
server running with id e1a4347ad6e7ce41cdcba728393d6dc6060b2ab0aeebeed1e9cb6e9aa235b6b2
```

(as client)
to connect to server, do
``cargo run --bin game_client e1a4347ad6e7ce41cdcba728393d6dc6060b2ab0aeebeed1e9cb6e9aa235b6b2``

or whatever the server id is
Then, when the game boots up, click "Start Client" to connect.