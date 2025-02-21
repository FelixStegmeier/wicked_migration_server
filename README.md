# Wicked Migration

Provides a webserver for migrating Wicked configuration files to NetworkManager configuration files.

## Build

`cargo build --release`

## Run Server

Requirements: Podman needs to be installed for the migration.

Options
| Option                | Description                                    |
|-----------------------|------------------------------------------------|
| `-s`, `--static-path` | Specify the directory for the static web files |
| `-i`, `--ip-address`  | Set IP address                                 |
| `-p`, `--port`        | Set port                                       |
| `-d`, `--db-path`     | Specify the location for the database          |
| `-h`, `--help`        | Show help information                          |

Configuration files are stored in the temp directory and are automatically deleted after 5 minutes or upon retrieval.

## Migration

Migration can be accessed through a web frontend or the commandline

### Commandline

With wicked:

```
sudo wicked show-config | \
     curl -v -L --output /path/to/output.tar \
     -H "Content-Type: application/xml" \
     --data-binary @- \
     http://<ip>:<port>
```

Single file:

```
curl -v -L --output /path/to/output.tar \
     -H "Content-Type: application/xml" \
     --data-binary "@/path/to/eth0.xml" \
     http://<ip>:<port>
```

Multiple files:

```
curl -v -L --output /path/to/output.tar \
    -F "file=@/path/to/ifcfg-eth0" \
    -F "file=@/path/to/ifcfg-eth1" \
    http://<ip>:<port>/multipart
```

### Web Frontend

The wicked migration server provides a simple frontend for migration, accessible through the browser at `http://<ipaddr>:<port>`
