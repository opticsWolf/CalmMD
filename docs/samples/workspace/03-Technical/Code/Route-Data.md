# Route data

The same sample checkpoint appears in three structured formats. Try changing the fence language in Source and comparing the result in Split.

## JSON

~~~json
{
  "route": "valley-loop",
  "distance_km": 7.2,
  "checkpoints": [
    { "name": "Old mill", "minute": 42, "water": false },
    { "name": "Meadow gate", "minute": 85, "water": true }
  ]
}
~~~

## YAML

~~~yaml
route: valley-loop
distance_km: 7.2
checkpoints:
  - name: Old mill
    minute: 42
    water: false
  - name: Meadow gate
    minute: 85
    water: true
~~~

## CSV

~~~csv
checkpoint,minute,water
Old mill,42,false
Meadow gate,85,true
~~~
