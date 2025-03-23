get_version() {
    local cargo_toml="contracts/$1/Cargo.toml"
    version=$(grep -m 1 "version" "$cargo_toml" | awk -F '"' '{print $2}')
    if [ ! -z "$version" ];then
        echo $version
    else
        # Echo version from root workspace Cargo.toml
        echo $(grep -m 1 "version" Cargo.toml | awk -F '"' '{print $2}')
    fi
}
