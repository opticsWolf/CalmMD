# Pace calculator

Each example calculates the average speed for a 9.6 km walk that takes 192 minutes.

## Rust

~~~rust
fn average_speed(distance_km: f64, minutes: f64) -> Option<f64> {
    (minutes > 0.0).then(|| distance_km / (minutes / 60.0))
}

fn main() {
    if let Some(speed) = average_speed(9.6, 192.0) {
        println!("{speed:.1} km/h");
    }
}
~~~

## Python

~~~python
def average_speed(distance_km: float, minutes: float) -> float:
    if minutes <= 0:
        raise ValueError("Minutes must be positive")
    return distance_km * 60 / minutes


print(f"{average_speed(9.6, 192):.1f} km/h")
~~~

## PowerShell

~~~powershell
$distanceKm = 9.6
$minutes = 192
'{0:0.0} km/h' -f ($distanceKm * 60 / $minutes)
~~~

## Output

~~~text
3.0 km/h
~~~

Back to [Distance and pace](../Distance-and-Pace.md).
