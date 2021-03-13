use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::Result;

use abstutil::{must_run_cmd, CmdArgs};

/// Import a one-shot A/B Street map in a single command. Takes a GeoJSON file with a boundary as
/// input. Automatically fetches the OSM data, clips it, and runs the importer.
/// TODO It currently overwrites a few fixed output files and doesn't clean them up.
fn main() -> Result<()> {
    let mut args = CmdArgs::new();
    let geojson_path = args.required_free();
    let drive_on_left = args.enabled("--drive_on_left");
    args.done();

    // TODO Assumes the binaries are in hardcoded target directories... we could be smarter and
    // detect if we're building from scratch, or look for stuff in the .zip release

    // Convert to a boundary polygon. This tool reads from STDIN.
    {
        println!("Converting GeoJSON to Osmosis boundary");
        let geojson = abstio::slurp_file(geojson_path)?;
        let mut cmd = Command::new("./target/debug/geojson_to_osmosis")
            .stdin(Stdio::piped())
            .spawn()?;
        let stdin = cmd.stdin.as_mut().unwrap();
        stdin.write_all(&geojson)?;
        assert!(cmd.wait()?.success());
    }

    // What file should we download?
    let url = {
        println!("Figuring out what Geofabrik file contains your boundary");
        let out = Command::new("./target/debug/pick_geofabrik")
            .arg("boundary0.poly")
            .output()?;
        assert!(out.status.success());
        String::from_utf8(out.stdout)?
    };

    // Download it!
    // TODO This is timing out. Also, really could use progress bars.
    {
        println!("Downloading {}", url);
        let resp = reqwest::blocking::get(&url)?;
        assert!(resp.status().is_success());
        let bytes = resp.bytes()?;
        let mut out = std::fs::File::create("raw.pbf")?;
        out.write_all(&bytes)?;
    }

    // Clip it
    println!("Clipping osm.pbf file to your boundary");
    must_run_cmd(
        Command::new("./target/release/clip_osm")
            .arg("--pbf=raw.pbf")
            .arg("--clip=boundary0.poly")
            .arg("--out=clipped.osm"),
    );

    // Import!
    {
        let mut cmd = Command::new("./target/release/importer");
        cmd.arg("--oneshot=clipped.osm");
        cmd.arg("--oneshot_clip=boundary0.poly");
        if drive_on_left {
            cmd.arg("--oneshot_drive_on_left");
        }
        println!("Running importer");
        must_run_cmd(&mut cmd);
    }

    Ok(())
}
