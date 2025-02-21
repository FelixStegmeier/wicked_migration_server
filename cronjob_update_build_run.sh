#!/bin/bash
repo="https://github.com/FelixStegmeier/wicked_migration_server.git"
server_dir="./wicked_migration_server"
migration_dir="$server_dir/wicked_migration_server"

git_has_changes() {
        echo fetching from remote $repo in $migration_dir
        git -C $migration_dir fetch origin
        main=$(git -C $migration_dir rev-parse main)
        echo comparing hashes
        echo "main              " $main
        origin_main=$(git -C $migration_dir rev-parse origin/main)
        echo "origin/main       " $origin_main
        [ $main != $origin_main ]
}

start_server_if_not_running() {
        pid=$(pidof wicked_migration_server)
        if [ -z "$pid" ]; then
                echo "starting server"
                "$server_dir"/release/wicked_migration_server -d $server_dir/db.db3 -s $server_dir/static -i :: -p 80
        fi
}

restart_server() {
        pid=$(pidof wicked_migration_server)
        if [ -n "$pid" ]; then
                echo "stopping server with pid $pid"
                kill "$pid"
        fi
        start_server_if_not_running
}

build() {
        cargo build --release --manifest-path $migration_dir/Cargo.toml --target-dir $server_dir
        cp -r $migration_dir/static $server_dir/static
}

if [ ! -d $server_dir ]; then
        echo "creating $server_dir"
        mkdir $server_dir
fi

if [ ! -d $migration_dir ]; then
        echo "cloning git repo"
        git clone $repo $migration_dir
        build
        start_server_if_not_running
fi

if git_has_changes; then
        echo "pulling new changes from remote $repo"
        git -C $migration_dir pull
        build
        restart_server
fi

start_server_if_not_running
